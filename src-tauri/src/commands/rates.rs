use sqlx::Row;
use tauri::State;

use crate::{db::AppState, models::*};

use super::{to_string, CommandResult};

#[tauri::command]
pub async fn list_cornice_rates(state: State<'_, AppState>) -> CommandResult<Vec<CorniceRate>> {
    let rows = sqlx::query(
        r#"
        SELECT id, series, model, unit
        FROM cornice_rates
        ORDER BY series COLLATE NOCASE, model COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(cornice_rate_from_row).collect())
}

#[tauri::command]
pub async fn save_cornice_rate(
    state: State<'_, AppState>,
    input: CorniceRateInput,
) -> CommandResult<CorniceRate> {
    if input.model.trim().is_empty() {
        return Err("Cornice model is required.".to_string());
    }
    let now = crate::db::now_string();
    let id = if let Some(id) = input.id {
        sqlx::query(
            r#"
            UPDATE cornice_rates
            SET series = ?, model = ?, unit = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.series.trim())
        .bind(input.model.trim())
        .bind(input.unit.trim())
        .bind(&now)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO cornice_rates
                (series, model, unit, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(input.series.trim())
        .bind(input.model.trim())
        .bind(input.unit.trim())
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    cornice_rate_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn delete_cornice_rate(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("DELETE FROM cornice_rates WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

// ==================== Fuzzy Search Cornice Rates ====================

#[tauri::command]
pub async fn search_cornice_rates(
    state: State<'_, AppState>,
    request: SearchCorniceRatesRequest,
) -> CommandResult<SearchCorniceRatesResponse> {
    let query = request.query.trim().to_ascii_lowercase();
    // Get all rates and do fuzzy matching in Rust (Levenshtein-like scoring)
    let rows = sqlx::query(
        r#"
        SELECT id, series, model, unit
        FROM cornice_rates
        ORDER BY model COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    let mut matches: Vec<CorniceRateMatch> = Vec::new();
    for row in rows {
        let model: String = row.get("model");
        let model_lower = model.to_ascii_lowercase();

        // Scoring: exact match, prefix, substring, then typo-tolerant
        // Levenshtein distance (allows up to max(1, len/3) edits).
        let score = if query.is_empty() {
            1
        } else if model_lower == query {
            1000
        } else if model_lower.starts_with(&query) {
            500
        } else if model_lower.contains(&query) {
            200
        } else {
            let distance = levenshtein(&model_lower, &query);
            let max_distance = (query.len() / 3).max(1);
            if distance <= max_distance {
                // distance >= 4 would underflow (150 - 160); saturate to the
                // minimum typo score of 10 instead.
                150u32.saturating_sub((distance as u32) * 40).max(10)
            } else {
                0
            }
        };

        if score > 0 {
            matches.push(CorniceRateMatch {
                id: row.get("id"),
                series: row.get("series"),
                model,
                unit: row.get("unit"),
                score,
            });
        }
    }

    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.model.cmp(&b.model)));
    matches.truncate(20);

    Ok(SearchCorniceRatesResponse { matches })
}

pub(crate) async fn cornice_rate_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<CorniceRate> {
    let row = sqlx::query(
        "SELECT id, series, model, unit FROM cornice_rates WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;

    Ok(cornice_rate_from_row(row))
}

pub(crate) async fn find_rate_for_model(
    db: &sqlx::SqlitePool,
    model: &str,
) -> Result<Option<CorniceRate>, sqlx::Error> {
    sqlx::query(
        r#"
        SELECT id, series, model, unit
        FROM cornice_rates
        WHERE lower(model) = lower(?)
        ORDER BY id
        LIMIT 1
        "#,
    )
    .bind(model)
    .fetch_optional(db)
    .await
    .map(|row| row.map(cornice_rate_from_row))
}

fn cornice_rate_from_row(row: sqlx::sqlite::SqliteRow) -> CorniceRate {
    CorniceRate {
        id: row.get("id"),
        series: row.get("series"),
        model: row.get("model"),
        unit: row.get("unit"),
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("491", "491"), 0);
        assert_eq!(levenshtein("491", "49"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("404", "440"), 2);
    }
}

