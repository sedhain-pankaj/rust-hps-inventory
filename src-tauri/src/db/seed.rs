use std::fs;

use anyhow::Result;
use sqlx::{Row, SqlitePool};


use super::migrate::log_migration_if_unapplied;
use super::util::{clean_cell, clean_series, expand_ambiguous, now_string, parse_csv};
use super::{set_permissions, AppPaths};

const CORNICE_RATE_CSV: &str = include_str!("../../../assets/cornice_rate.csv");
const OVERALL_STOCK_CSV: &str = include_str!("../../../assets/overall_stock.csv");
const HPS_LOGO: &[u8] = include_bytes!("../../../assets/HPS.png");
const LEGACY_ADMIN_HASH: &str = "8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918";

pub(crate) async fn seed_assets(db: &SqlitePool) -> Result<()> {
    let now = now_string();
    for (key, name, media_type, content) in [
        (
            "hps_logo",
            "Hopkins Plaster Studio logo",
            "image/png",
            HPS_LOGO.to_vec(),
        ),
        (
            "cornice_rate_csv",
            "Seed cornice rates CSV",
            "text/csv",
            CORNICE_RATE_CSV.as_bytes().to_vec(),
        ),
        (
            "overall_stock_csv",
            "Seed overall stock CSV",
            "text/csv",
            OVERALL_STOCK_CSV.as_bytes().to_vec(),
        ),
    ] {
        sqlx::query(
            r#"
            INSERT INTO app_assets (key, name, media_type, content, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                name = excluded.name,
                media_type = excluded.media_type,
                content = excluded.content,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(key)
        .bind(name)
        .bind(media_type)
        .bind(content)
        .bind(&now)
        .execute(db)
        .await?;
    }
    Ok(())
}

pub(crate) async fn seed_if_needed(db: &SqlitePool, paths: &AppPaths) -> Result<()> {
    let seeded: Option<String> =
        sqlx::query("SELECT value FROM app_meta WHERE key = 'seed_version'")
            .fetch_optional(db)
            .await?
            .map(|row| row.get("value"));

    if seeded.is_some() {
        return Ok(());
    }

    seed_default_employees(db).await?;
    seed_cornice_rates(db).await?;
    seed_stock_items(db).await?;
    import_legacy_employees_if_present(db, paths).await?;
    import_legacy_fingerprints_if_present(db, paths).await?;
    import_legacy_clock_events_if_present(db, paths).await?;

    sqlx::query("INSERT OR REPLACE INTO app_meta (key, value) VALUES ('seed_version', '1')")
        .execute(db)
        .await?;
    Ok(())
}

// One-shot: the Mould Locations staff tab became permission-driven (mould_view).
// Grants it to the roles that had the tab before (storekeeper/driver/helper) so
// nobody loses access; afterwards the admin's checkbox is the only control.
pub(crate) async fn backfill_mould_view_permissions(db: &SqlitePool) -> Result<()> {
    let applied = log_migration_if_unapplied(db, "grant_mould_view_to_stock_roles").await?;
    if !applied {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO employee_permissions (employee_id, permission)
        SELECT id, 'mould_view' FROM employees
        WHERE staff_category IN ('storekeeper', 'driver', 'helper')
        "#,
    )
    .execute(db)
    .await?;
    Ok(())
}

// One-shot: give every location the default R1..R5 columns, renaming the
// legacy "Column 1" seeded by the earlier migration to R1. Logged, so columns
// the admin deletes afterwards are not resurrected on a later start.
pub(crate) async fn seed_default_mould_columns(db: &SqlitePool) -> Result<()> {
    let applied = log_migration_if_unapplied(db, "seed_mould_columns_r1_r5").await?;
    if !applied {
        return Ok(());
    }
    let location_ids: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM mould_locations").fetch_all(db).await?;
    for (location_id,) in location_ids {
        let cols: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, name FROM mould_location_columns WHERE location_id = ? ORDER BY sort_order, id",
        )
        .bind(location_id)
        .fetch_all(db)
        .await?;
        if cols.is_empty() {
            for index in 1..=5 {
                sqlx::query(
                    "INSERT INTO mould_location_columns (location_id, name, sort_order) VALUES (?, ?, ?)",
                )
                .bind(location_id)
                .bind(format!("R{index}"))
                .bind(index - 1)
                .execute(db)
                .await?;
            }
            continue;
        }
        let renamed = cols
            .first()
            .map(|(_, name)| name == "Column 1")
            .unwrap_or(false);
        if renamed {
            if let Some((first_id, _)) = cols.first() {
                sqlx::query("UPDATE mould_location_columns SET name = 'R1' WHERE id = ?")
                    .bind(first_id)
                    .execute(db)
                    .await?;
            }
        }
        // Re-read names so numbering below sees the post-rename state (a
        // location whose only column was just renamed to R1 must not get R1
        // inserted again).
        let names: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM mould_location_columns WHERE location_id = ? ORDER BY sort_order, id",
        )
        .bind(location_id)
        .fetch_all(db)
        .await?;
        if names.len() < 5 {
            let mut max_index = 0;
            for (name,) in &names {
                if let Some(digits) = name.strip_prefix('R') {
                    if let Ok(value) = digits.parse::<i64>() {
                        max_index = max_index.max(value);
                    }
                }
            }
            let mut sort_order = names.len() as i64;
            for index in (max_index + 1)..=5 {
                sqlx::query(
                    "INSERT INTO mould_location_columns (location_id, name, sort_order) VALUES (?, ?, ?)",
                )
                .bind(location_id)
                .bind(format!("R{index}"))
                .bind(sort_order)
                .execute(db)
                .await?;
                sort_order += 1;
            }
        }
    }
    Ok(())
}

async fn seed_default_employees(db: &SqlitePool) -> Result<()> {
    let now = now_string();
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO employees
            (id, name, finger, active, is_admin, password_hash, created_at, updated_at)
        VALUES
            ('EMP001', 'Admin', 'right-index', 1, 1, ?, ?, ?)
        "#,
    )
    .bind(LEGACY_ADMIN_HASH)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    set_permissions(
        db,
        "EMP001",
        &[
            "clock",
            "cornice_log",
            "production_log",
            "overstock",
            "deliveries",
            "cornice_rates_view",
            "daily_production_all",
        ],
    )
    .await?;

    Ok(())
}

async fn seed_cornice_rates(db: &SqlitePool) -> Result<()> {
    let rows = parse_csv(CORNICE_RATE_CSV);
    if rows.is_empty() {
        return Ok(());
    }

    let headers = &rows[0];
    let now = now_string();
    for row in rows.iter().skip(1) {
        let mut index = 0;
        while index + 1 < headers.len() {
            let series = clean_series(headers.get(index).cloned().unwrap_or_default());
            let model = row.get(index).map(clean_cell).unwrap_or_default();
            let unit = row.get(index + 1).map(clean_cell).unwrap_or_default();
            if !series.is_empty() && !model.is_empty() {
                for (m, u) in expand_ambiguous(&model, &unit) {
                    sqlx::query(
                        r#"
                        INSERT OR IGNORE INTO cornice_rates
                            (series, model, unit, updated_at)
                        VALUES (?, ?, ?, ?)
                        "#,
                    )
                    .bind(&series)
                    .bind(&m)
                    .bind(&u)
                    .bind(&now)
                    .execute(db)
                    .await?;
                }
            }
            index += 2;
        }
    }
    Ok(())
}

async fn seed_stock_items(db: &SqlitePool) -> Result<()> {
    let rows = parse_csv(OVERALL_STOCK_CSV);
    let now = now_string();
    for row in rows.iter().skip(1) {
        let model = row.first().map(clean_cell).unwrap_or_default();
        if model.is_empty() {
            continue;
        }
        let stock = row
            .get(1)
            .map(|value| clean_cell(value).parse::<i64>().unwrap_or(0))
            .unwrap_or(0);
        let location = row.get(2).map(clean_cell).unwrap_or_default();
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO stock_items
                (item_type, model, stock, location, updated_at)
            VALUES ('cornice', ?, ?, ?, ?)
            "#,
        )
        .bind(model)
        .bind(stock)
        .bind(location)
        .bind(&now)
        .execute(db)
        .await?;
    }
    Ok(())
}

async fn import_legacy_employees_if_present(db: &SqlitePool, paths: &AppPaths) -> Result<()> {
    let path = paths.source_root.join("data").join("employees.csv");
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };

    for row in parse_csv(&content).iter().skip(1) {
        let id = row.first().map(clean_cell).unwrap_or_default();
        let name = row.get(1).map(clean_cell).unwrap_or_default();
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let finger = row
            .get(2)
            .map(clean_cell)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "right-index".to_string());
        let active = row
            .get(4)
            .map(clean_cell)
            .map(|value| !matches!(value.as_str(), "0" | "false" | "False" | "no"))
            .unwrap_or(true);
        let now = now_string();
        sqlx::query(
            r#"
            INSERT INTO employees (id, name, finger, active, is_admin, password_hash, created_at, updated_at)
            VALUES (?, ?, ?, ?, CASE WHEN ? = 'EMP001' THEN 1 ELSE 0 END,
                    CASE WHEN ? = 'EMP001' THEN ? ELSE NULL END, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                finger = excluded.finger,
                active = excluded.active,
                is_admin = CASE WHEN excluded.id = 'EMP001' THEN 1 ELSE employees.is_admin END,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(finger)
        .bind(active as i64)
        .bind(&id)
        .bind(&id)
        .bind(LEGACY_ADMIN_HASH)
        .bind(&now)
        .bind(&now)
        .execute(db)
        .await?;

        if id == "EMP001" {
            set_permissions(
                db,
                &id,
                &[
                    "clock",
                    "cornice_log",
                    "production_log",
                    "overstock",
                    "deliveries",
                    "cornice_rates_view",
                    "daily_production_all",
                ],
            )
            .await?;
        } else if id == "EMP002" {
            set_permissions(db, &id, &["clock", "cornice_log", "cornice_rates_view"]).await?;
        } else {
            set_permissions(db, &id, &["clock", "production_log"]).await?;
        }
    }

    Ok(())
}

async fn import_legacy_clock_events_if_present(db: &SqlitePool, paths: &AppPaths) -> Result<()> {
    let path = paths.source_root.join("data").join("time_clock_log.csv");
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };

    for row in parse_csv(&content).iter().skip(1) {
        let timestamp = row.first().map(clean_cell).unwrap_or_default();
        let employee_id = row.get(1).map(clean_cell).unwrap_or_default();
        let action = row.get(3).map(clean_cell).unwrap_or_default();
        let source = row
            .get(4)
            .map(clean_cell)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "fingerprint".to_string());
        if timestamp.len() < 10 || employee_id.is_empty() {
            continue;
        }
        let work_date = timestamp[..10].to_string();
        sqlx::query(
            r#"
            INSERT INTO time_clock_events
                (employee_id, work_date, action, timestamp, source, needs_admin_review, note)
            VALUES (?, ?, ?, ?, ?, 0, '')
            "#,
        )
        .bind(employee_id)
        .bind(work_date)
        .bind(action)
        .bind(timestamp)
        .bind(source)
        .execute(db)
        .await
        .ok();
    }

    Ok(())
}

async fn import_legacy_fingerprints_if_present(db: &SqlitePool, paths: &AppPaths) -> Result<()> {
    let rows = sqlx::query("SELECT id, finger FROM employees")
        .fetch_all(db)
        .await?;
    let now = now_string();

    for row in rows {
        let employee_id: String = row.get("id");
        let finger: String = row.get("finger");
        let path = paths
            .source_root
            .join("data")
            .join("fingerprints")
            .join(format!("{employee_id}.fpdata"));
        let Ok(template) = fs::read(path) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO fingerprint_templates (employee_id, finger, template, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(employee_id) DO UPDATE SET
                finger = excluded.finger,
                template = excluded.template,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(employee_id)
        .bind(finger)
        .bind(template)
        .bind(&now)
        .execute(db)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::migrate::{migrate, run_column_migrations, run_data_migrations, run_cornice_unit_migrations};
    use super::super::util::now_string;
    use super::*;
    use sqlx::sqlite::{SqlitePoolOptions, SqlitePool};

async fn fresh_pool() -> SqlitePool {
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    async fn run_all_migrations(pool: &SqlitePool) {
        migrate(pool).await.unwrap();
        run_column_migrations(pool).await.unwrap();
        run_data_migrations(pool).await.unwrap();
        run_cornice_unit_migrations(pool).await.unwrap();
    }
    #[tokio::test]
    async fn cornice_stock_rows_copy_into_stock_items_without_duplicates() {
        let pool = fresh_pool().await;
        run_all_migrations(&pool).await;

        sqlx::query(
            "INSERT INTO cornice_stock (model, aisle, quantity_in_stock, quantity_reserved, remarks, updated_at)
             VALUES ('M1', 'Aisle 1', 5, 2, 'note', '2026-01-01T00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        run_data_migrations(&pool).await.unwrap();
        run_data_migrations(&pool).await.unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_items WHERE item_type = 'cornice' AND model = 'M1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);

        let (stock, reserved, location): (i64, i64, String) =
            sqlx::query_as("SELECT stock, reserved, location FROM stock_items WHERE model = 'M1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stock, 5);
        assert_eq!(reserved, 2);
        assert_eq!(location, "Aisle 1");
    }

    #[tokio::test]
    async fn mould_column_seed_renames_legacy_and_tops_up_to_r5() {
        let pool = fresh_pool().await;
        run_all_migrations(&pool).await;

        // Simulate the state left by the earlier migration version: a
        // "Column 1" per location, plus one user-added R2.
        let singles: i64 = sqlx::query_scalar(
            "SELECT id FROM mould_locations WHERE name = 'Singles Wall'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let near: i64 = sqlx::query_scalar(
            "SELECT id FROM mould_locations WHERE name = 'Near Dryer'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM mould_location_columns")
            .execute(&pool)
            .await
            .unwrap();
        for location_id in [singles, near] {
            sqlx::query(
                "INSERT INTO mould_location_columns (location_id, name, sort_order) VALUES (?, 'Column 1', 0)",
            )
            .bind(location_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO mould_location_columns (location_id, name, sort_order) VALUES (?, 'R2', 1)",
        )
        .bind(singles)
        .execute(&pool)
        .await
        .unwrap();

        // Re-arm the one-shot seed and run it.
        sqlx::query(
            "DELETE FROM _schema_migration_log WHERE migration_id = 'seed_mould_columns_r1_r5'",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_default_mould_columns(&pool).await.unwrap();

        for location_id in [singles, near] {
            let cols: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM mould_location_columns WHERE location_id = ? ORDER BY sort_order",
            )
            .bind(location_id)
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(cols, vec!["R1", "R2", "R3", "R4", "R5"]);
        }
    }

    #[tokio::test]
    async fn mould_column_delete_only_allows_last_column() {
        use crate::commands::delete_mould_location_column_checked as del;

        let pool = fresh_pool().await;
        run_all_migrations(&pool).await;

        let loc: i64 = sqlx::query_scalar(
            "SELECT id FROM mould_locations WHERE name = 'Singles Wall'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cols: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, name FROM mould_location_columns WHERE location_id = ? ORDER BY sort_order",
        )
        .bind(loc)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(cols.len(), 5);
        let r4 = cols[3].0;
        let r5 = cols[4].0;

        // R4 is not the last column -> rejected.
        let err = del(&pool, r4)
            .await
            .err()
            .expect("deleting a non-last column must fail");
        assert!(err.contains("Only the last column"), "unexpected: {err}");

        // R5 is the last column and empty -> allowed.
        del(&pool, r5)
            .await
            .expect("deleting the last empty column must succeed");

        // Now R4 is the last column -> allowed.
        del(&pool, r4)
            .await
            .expect("deleting the new last empty column must succeed");

        // A non-empty last column is allowed: its moulds are unassigned.
        let last: i64 = sqlx::query_scalar(
            "SELECT id FROM mould_location_columns WHERE location_id = ? ORDER BY sort_order DESC, id DESC LIMIT 1",
        )
        .bind(loc)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO mould_inventory (mould_name, storage_location, column_id, updated_at)
             VALUES ('Test Mould', 'Singles Wall', ?, ?)",
        )
        .bind(last)
        .bind(now_string())
        .execute(&pool)
        .await
        .unwrap();
        del(&pool, last)
            .await
            .expect("deleting a non-empty column must succeed");
        let unassigned: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mould_inventory WHERE mould_name = 'Test Mould' AND column_id IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(unassigned, 1, "mould must be unassigned after column delete");
    }

    #[tokio::test]
    async fn mould_view_backfill_grants_stock_roles_once() {
        let pool = fresh_pool().await;
        run_all_migrations(&pool).await;

        let now = now_string();
        for (id, category) in [
            ("S1", "storekeeper"),
            ("D1", "driver"),
            ("H1", "helper"),
            ("C1", "cornice_hand"),
        ] {
            sqlx::query(
                "INSERT INTO employees (id, name, finger, active, is_admin, password_hash, created_at, updated_at, staff_category)
                 VALUES (?, ?, 'right-index', 1, 0, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(id)
            .bind(LEGACY_ADMIN_HASH)
            .bind(&now)
            .bind(&now)
            .bind(category)
            .execute(&pool)
            .await
            .unwrap();
        }

        backfill_mould_view_permissions(&pool).await.unwrap();
        // Running again must not re-grant (one-shot, logged).
        backfill_mould_view_permissions(&pool).await.unwrap();

        for (id, expected) in [("S1", 1i64), ("D1", 1), ("H1", 1), ("C1", 0)] {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM employee_permissions WHERE employee_id = ? AND permission = 'mould_view'",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, expected, "{id} mould_view backfill mismatch");
        }
    }
}
