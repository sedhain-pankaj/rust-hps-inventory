use sqlx::Row;
use tauri::State;

use crate::db::{
    employee_by_id, list_employees, notification, AppState,
};
use crate::models::*;

use super::logs::seconds_from_event_rows;
use super::rates::cornice_rate_by_id;
use super::{to_string, CommandResult};

// ==================== Payroll Engine ====================

/// One cornice log row as needed for payroll math (DB-free for testability).
struct CorniceRowForPay {
    model: String,
    lengths: i64,
    unit_value: Option<f64>,
    is_custom: bool,
}

/// Result of the pure payroll computation for one employee-week.
struct PayrollMath {
    total_units_known: f64,
    total_units_unknown: f64,
    unknown_details: Vec<UnknownRateDetail>,
    unit_threshold: f64,
    threshold_note: String,
    base_pay: f64,
    gross_pay: Option<f64>,
    extra_unit_pay: f64,
    pay_equation: String,
    status: String,
}

/// Base pay rate per hour worked. A full 40-hr week earns 40 × $28.50 = $1140.
const BASE_RATE_PER_HOUR: f64 = 28.50;
/// Units produced per hour worked (36 units / 8-hr day). To earn the base rate
/// you must average this many units per hour; the base-pay unit threshold for a
/// week is `UNITS_PER_HOUR * hours_worked` (a full 40-hr week → 180 units).
const UNITS_PER_HOUR: f64 = 4.5;
/// A "standard" week is 40 hours (5 days × 8 hr). Used when an admin overrides
/// a week to standard (e.g. the clocked hours are wrong).
const STANDARD_WEEK_HOURS: f64 = 40.0;
/// Hours band considered a "normal" full week. A week outside this band has an
/// unusual hours-based proration and is flagged for admin review.
const MIN_NORMAL_HOURS: f64 = 39.0;
const MAX_NORMAL_HOURS: f64 = 41.0;

/// Round to cents — keeps stored/sent money values free of IEEE-754 noise
/// (e.g. 209.9 - 180 = 29.900000000000006).
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// True when a week's hours fall outside the normal 39-41 band, meaning the
/// hours-based proration is unusual enough to warrant admin review.
fn hours_need_review(hours: f64) -> bool {
    hours < MIN_NORMAL_HOURS || hours > MAX_NORMAL_HOURS
}

/// Final status for a computed week. Unknown rates always win ("unresolved").
/// A week an admin has already reviewed (or force-overridden) is "final".
/// Otherwise a week whose hours fall outside the normal 39-41 band is flagged
/// "review" so an admin can confirm the proration.
fn final_status(math_status: &str, total_hours: f64, reviewed: bool) -> String {
    if math_status == "unresolved" {
        "unresolved".to_string()
    } else if reviewed {
        "final".to_string()
    } else if hours_need_review(total_hours) {
        "review".to_string()
    } else {
        "final".to_string()
    }
}

/// Pure payroll math: splits cornice rows into known units and unknown-rate
/// details, then computes pay. Both base pay and the base-pay unit threshold
/// scale linearly with `effective_hours` (the hours to prorate by — the clocked
/// hours, or 40 when an admin overrode the week to standard):
///   base_pay  = $28.50 × effective_hours
///   threshold = 4.5    × effective_hours
/// The first `threshold` made units are covered by base pay; each unit above
/// earns the extra unit rate. Any custom or unknown-rate row makes the week
/// "unresolved" until an admin sets the rate.
fn compute_payroll(rows: &[CorniceRowForPay], effective_hours: f64, standard_week: bool) -> PayrollMath {
    const EXTRA_UNIT_RATE: f64 = 3.80;

    let base_pay = round2(BASE_RATE_PER_HOUR * effective_hours);
    let threshold = round2(UNITS_PER_HOUR * effective_hours);

    let mut total_units_known = 0.0_f64;
    let mut unknown_details: Vec<UnknownRateDetail> = Vec::new();
    for row in rows {
        if row.is_custom || row.unit_value.is_none() {
            // Merge with existing unknown detail for same model
            if let Some(existing) = unknown_details.iter_mut().find(|d| d.model == row.model) {
                existing.quantity += row.lengths;
            } else {
                unknown_details.push(UnknownRateDetail {
                    model: row.model.clone(),
                    quantity: row.lengths,
                });
            }
        } else if let Some(uv) = row.unit_value {
            total_units_known += uv * row.lengths as f64;
        }
    }
    let total_units_known = round2(total_units_known);
    let total_units_unknown: f64 = unknown_details.iter().map(|d| d.quantity as f64).sum();

    let threshold_note = if standard_week {
        format!("Standard {:.0}-hr week (admin override)", STANDARD_WEEK_HOURS)
    } else {
        format!("4.5 units/hr × {:.1}h worked", effective_hours)
    };

    let (gross_pay, extra_unit_pay, pay_equation, status) = if !unknown_details.is_empty() {
        // Unknown rate equation (§5.3)
        let known_part = total_units_known.floor() as i64;
        let unknown_parts: Vec<String> = unknown_details
            .iter()
            .map(|d| format!("{}×{}", d.quantity, d.model))
            .collect();
        let eq = format!(
            "{} units + {} − {:.0} (base units)",
            known_part,
            unknown_parts.join(" + "),
            threshold
        );
        (None, 0.0, eq, "unresolved".to_string())
    } else {
        let extra_units = round2((total_units_known - threshold).max(0.0));
        let eup = round2(extra_units * EXTRA_UNIT_RATE);
        let gp = round2(base_pay + eup);
        let eq = format!(
            "${:.2} (base) + ${:.2} ({:.0} extra units × $3.80) = ${:.2}",
            base_pay, eup, extra_units, gp
        );
        (Some(gp), eup, eq, "final".to_string())
    };

    PayrollMath {
        total_units_known,
        total_units_unknown,
        unknown_details,
        unit_threshold: threshold,
        threshold_note,
        base_pay,
        gross_pay,
        extra_unit_pay,
        pay_equation,
        status,
    }
}

fn cornice_rows_for_pay(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<CorniceRowForPay> {
    rows.iter()
        .map(|row| CorniceRowForPay {
            model: row.get("model"),
            lengths: row.get("lengths"),
            unit_value: row.get("unit_value"),
            is_custom: row.get::<i64, _>("is_custom") != 0,
        })
        .collect()
}

/// Read the admin proration controls for a week. Returns `(use_standard_week,
/// reviewed)` — both default to false when the period row doesn't exist yet.
async fn read_payroll_flags(
    db: &sqlx::SqlitePool,
    employee_id: &str,
    week_start: &str,
) -> Result<(bool, bool), String> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT use_standard_week, reviewed FROM payroll_periods \
         WHERE employee_id = ? AND week_start = ?",
    )
    .bind(employee_id)
    .bind(week_start)
    .fetch_optional(db)
    .await
    .map_err(to_string)?;
    Ok(match row {
        Some((std, rev)) => (std != 0, rev != 0),
        None => (false, false),
    })
}

/// Compute the payroll response for one employee-week, honouring any admin
/// standard-week override / review flag. Pure read — does not persist.
async fn compute_week(
    db: &sqlx::SqlitePool,
    employee_id: &str,
    employee_name: &str,
    week_start: chrono::NaiveDate,
    use_standard_week: bool,
    reviewed: bool,
) -> Result<PayrollWeekResponse, String> {
    let week_end = week_start + chrono::Duration::days(6);

    let clock_rows = sqlx::query(
        r#"
        SELECT * FROM time_clock_events
        WHERE employee_id = ? AND work_date >= ? AND work_date <= ?
        ORDER BY timestamp ASC, id ASC
        "#,
    )
    .bind(employee_id)
    .bind(week_start.format("%Y-%m-%d").to_string())
    .bind(week_end.format("%Y-%m-%d").to_string())
    .fetch_all(db)
    .await
    .map_err(to_string)?;

    let (total_seconds, ..) = seconds_from_event_rows(&clock_rows, false);
    let total_hours = total_seconds as f64 / 3600.0;

    let cornice_rows = sqlx::query(
        r#"
        SELECT model, lengths, unit_value, is_custom
        FROM cornice_logs
        WHERE employee_id = ? AND week_start = ?
        ORDER BY id ASC
        "#,
    )
    .bind(employee_id)
    .bind(week_start.format("%Y-%m-%d").to_string())
    .fetch_all(db)
    .await
    .map_err(to_string)?;

    // Prorate by the clocked hours, or a standard 40-hr week when an admin has
    // overridden the week (e.g. the clock data is wrong). The response still
    // reports the clocked `total_hours` so the discrepancy is visible.
    let effective_hours = if use_standard_week {
        STANDARD_WEEK_HOURS
    } else {
        total_hours
    };
    let math = compute_payroll(
        &cornice_rows_for_pay(&cornice_rows),
        effective_hours,
        use_standard_week,
    );
    let status = final_status(&math.status, total_hours, reviewed);
    let needs_admin_review = status == "unresolved" || status == "review";

    Ok(PayrollWeekResponse {
        employee_id: employee_id.to_string(),
        employee_name: employee_name.to_string(),
        week_start: week_start.format("%Y-%m-%d").to_string(),
        week_end: week_end.format("%Y-%m-%d").to_string(),
        total_hours,
        total_units_known: math.total_units_known,
        total_units_unknown: math.total_units_unknown,
        unknown_rate_details: math.unknown_details,
        unit_threshold: math.unit_threshold,
        threshold_note: math.threshold_note,
        base_pay: math.base_pay,
        extra_unit_pay: math.extra_unit_pay,
        gross_pay: math.gross_pay,
        pay_equation: math.pay_equation,
        needs_admin_review,
        status,
    })
}

/// Persist (insert or refresh) the payroll period row for a computed week.
async fn upsert_payroll_period(
    db: &sqlx::SqlitePool,
    response: &PayrollWeekResponse,
    use_standard_week: bool,
    reviewed: bool,
) -> Result<(), String> {
    let gross_for_db = response.gross_pay.unwrap_or(0.0);
    sqlx::query(
        r#"
        INSERT INTO payroll_periods
            (employee_id, week_start, week_end, total_hours, total_units_known, unit_threshold,
             base_pay, extra_unit_pay, gross_pay, status, unknown_rate_equation, needs_admin_review,
             use_standard_week, reviewed, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(employee_id, week_start) DO UPDATE SET
            week_end = excluded.week_end,
            total_hours = excluded.total_hours,
            total_units_known = excluded.total_units_known,
            unit_threshold = excluded.unit_threshold,
            base_pay = excluded.base_pay,
            extra_unit_pay = excluded.extra_unit_pay,
            gross_pay = excluded.gross_pay,
            status = excluded.status,
            unknown_rate_equation = excluded.unknown_rate_equation,
            needs_admin_review = excluded.needs_admin_review,
            use_standard_week = excluded.use_standard_week,
            reviewed = excluded.reviewed,
            created_at = excluded.created_at
        "#,
    )
    .bind(&response.employee_id)
    .bind(&response.week_start)
    .bind(&response.week_end)
    .bind(response.total_hours)
    .bind(response.total_units_known)
    .bind(response.unit_threshold)
    .bind(response.base_pay)
    .bind(response.extra_unit_pay)
    .bind(gross_for_db)
    .bind(&response.status)
    .bind(&response.pay_equation)
    .bind(response.needs_admin_review as i64)
    .bind(use_standard_week as i64)
    .bind(reviewed as i64)
    .bind(crate::db::now_string())
    .execute(db)
    .await
    .map_err(to_string)?;
    Ok(())
}

/// Raise a proration review alert, but only if one for this employee+week isn't
/// already pending — the staff payroll view recomputes (and would re-raise) on
/// every open, so without this the admin would be spammed with duplicates.
async fn raise_proration_alert(
    db: &sqlx::SqlitePool,
    employee_name: &str,
    week_start_str: &str,
    total_hours: f64,
    threshold: f64,
) {
    let existing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_notifications \
         WHERE kind = 'payroll_proration' AND resolved = 0 AND message LIKE ? AND message LIKE ?",
    )
    .bind(format!("%{}%", employee_name))
    .bind(format!("%{}%", week_start_str))
    .fetch_one(db)
    .await
    .unwrap_or(0);
    if existing > 0 {
        return;
    }
    notification(
        db,
        "yellow",
        "payroll_proration",
        &format!(
            "{} worked {:.1} hrs for week {} (outside 39-41 band). Prorated threshold: {:.0} units. Admin review needed.",
            employee_name, total_hours, week_start_str, threshold
        ),
        "payroll_periods",
        None,
    )
    .await
    .ok();
}

#[tauri::command]
pub async fn get_payroll_week(
    state: State<'_, AppState>,
    request: PayrollWeekRequest,
) -> CommandResult<PayrollWeekResponse> {
    let employee = employee_by_id(&state.db, &request.employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee not found.".to_string())?;

    let now_date = chrono::Local::now().date_naive();
    let req_week_start = request.week_start.as_ref()
        .and_then(|ws| chrono::NaiveDate::parse_from_str(ws, "%Y-%m-%d").ok())
        .unwrap_or_else(|| crate::db::week_start_for(now_date));

    let week_start_str = req_week_start.format("%Y-%m-%d").to_string();

    let (use_standard_week, reviewed) =
        read_payroll_flags(&state.db, &employee.id, &week_start_str).await?;
    let response = compute_week(
        &state.db,
        &employee.id,
        &employee.name,
        req_week_start,
        use_standard_week,
        reviewed,
    )
    .await?;

    // Persist payroll period (preserving any admin standard-week override / review).
    upsert_payroll_period(&state.db, &response, use_standard_week, reviewed).await?;

    // Raise alerts for unresolved or outside-band (unreviewed) weeks.
    if response.status == "unresolved" {
        notification(
            &state.db,
            "red",
            "payroll_unresolved",
            &format!("{} has unknown-rate cornice units for week {}. Pay cannot be finalized.", employee.name, week_start_str),
            "payroll_periods",
            None,
        )
        .await
        .ok();
    } else if response.status == "review" {
        raise_proration_alert(
            &state.db,
            &employee.name,
            &week_start_str,
            response.total_hours,
            response.unit_threshold,
        )
        .await;
    }

    Ok(response)
}

#[tauri::command]
pub async fn get_all_payroll_week(
    state: State<'_, AppState>,
    request: AdminPayrollWeekRequest,
) -> CommandResult<Vec<PayrollWeekResponse>> {
    let now_date = chrono::Local::now().date_naive();
    let req_week_start = request.week_start.as_ref()
        .and_then(|ws| chrono::NaiveDate::parse_from_str(ws, "%Y-%m-%d").ok())
        .unwrap_or_else(|| crate::db::week_start_for(now_date));

    let employees = list_employees(&state.db, true).await.map_err(to_string)?;
    let week_start_str = req_week_start.format("%Y-%m-%d").to_string();
    let mut results = Vec::new();

    for employee in employees {
        let (use_standard_week, reviewed) =
            match read_payroll_flags(&state.db, &employee.id, &week_start_str).await {
                Ok(flags) => flags,
                Err(_) => continue,
            };
        let result = compute_week(
            &state.db,
            &employee.id,
            &employee.name,
            req_week_start,
            use_standard_week,
            reviewed,
        )
        .await;
        if let Ok(r) = result {
            results.push(r);
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn resolve_unknown_rate(
    state: State<'_, AppState>,
    input: ResolveUnknownRateInput,
) -> CommandResult<CorniceRate> {
    let model = input.model.trim();
    if model.is_empty() {
        return Err("Model name is required.".to_string());
    }
    if input.unit_value <= 0.0 {
        return Err("Unit value must be positive.".to_string());
    }

    let now = crate::db::now_string();
    let series = input.series.as_deref().unwrap_or_default().trim().to_string();

    // Check if rate already exists
    let existing = sqlx::query(
        r#"
        SELECT id FROM cornice_rates
        WHERE lower(model) = lower(?)
        "#,
    )
    .bind(model)
    .fetch_optional(&state.db)
    .await
    .map_err(to_string)?;

    let id = if let Some(row) = existing {
        let existing_id: i64 = row.get("id");
        sqlx::query(
            r#"
            UPDATE cornice_rates
            SET unit = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(format!("{:.2}", input.unit_value))
        .bind(&now)
        .bind(existing_id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        existing_id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO cornice_rates (series, model, unit, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&series)
        .bind(model)
        .bind(format!("{:.2}", input.unit_value))
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    // Update cornice_logs that had this model as custom/unknown — recalculate units
    sqlx::query(
        r#"
        UPDATE cornice_logs
        SET unit_value = ?, total_units = ROUND(lengths * ?, 2), is_custom = 0, needs_admin_review = 0, updated_at = ?
        WHERE lower(model) = lower(?) AND (is_custom = 1 OR unit_value IS NULL)
        "#,
    )
    .bind(input.unit_value)
    .bind(input.unit_value)
    .bind(&now)
    .bind(model)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    // Resolve related payroll alerts
    sqlx::query(
        r#"
        UPDATE admin_notifications
        SET resolved = 1
        WHERE kind = 'payroll_unresolved'
          AND message LIKE ?
          AND resolved = 0
        "#,
    )
    .bind(format!("%{}%", model))
    .execute(&state.db)
    .await
    .ok();

    cornice_rate_by_id(&state.db, id).await
}

// ==================== Payroll Proration Override ====================

#[tauri::command]
pub async fn override_payroll_proration(
    state: State<'_, AppState>,
    input: OverridePayrollProrationInput,
) -> CommandResult<PayrollWeekResponse> {
    let employee_id = input.employee_id.trim();
    let week_start_str = input.week_start.trim();

    let employee = employee_by_id(&state.db, employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee not found.".to_string())?;

    let week_start = chrono::NaiveDate::parse_from_str(week_start_str, "%Y-%m-%d")
        .map_err(|_| "Invalid week_start date format. Use YYYY-MM-DD.".to_string())?;

    // accept_prorated = true  -> use the clocked hours (hours-based proration)
    // accept_prorated = false -> treat the week as a standard 40-hr week
    let use_standard_week = !input.accept_prorated;

    // Recompute with the admin's decision (reviewed = true so the week is final),
    // then persist it so the override/review sticks across future recomputes.
    let response = compute_week(
        &state.db,
        employee_id,
        &employee.name,
        week_start,
        use_standard_week,
        true,
    )
    .await?;
    upsert_payroll_period(&state.db, &response, use_standard_week, true).await?;

    // Resolve the related proration alert (message carries the employee name).
    sqlx::query(
        r#"
        UPDATE admin_notifications
        SET resolved = 1
        WHERE kind = 'payroll_proration'
          AND message LIKE ?
          AND resolved = 0
        "#,
    )
    .bind(format!("%{}%", employee.name))
    .execute(&state.db)
    .await
    .ok();

    Ok(response)
}

#[cfg(test)]
mod payroll_math_tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ≈ {b}");
    }

    fn row(model: &str, lengths: i64, unit_value: Option<f64>, is_custom: bool) -> CorniceRowForPay {
        CorniceRowForPay {
            model: model.to_string(),
            lengths,
            unit_value,
            is_custom,
        }
    }

    #[test]
    fn below_threshold_pays_base_only() {
        // 100 lengths × 1.5 = 150 known units (< 180)
        let math = compute_payroll(&[row("404", 100, Some(1.5), false)], 40.0, false);
        close(math.total_units_known, 150.0);
        close(math.unit_threshold, 180.0);
        close(math.extra_unit_pay, 0.0);
        close(math.gross_pay.unwrap(), 1140.0);
        assert_eq!(math.status, "final");
        assert!(math.unknown_details.is_empty());
    }

    #[test]
    fn exactly_at_threshold_pays_base_only() {
        // 120 lengths × 1.5 = 180 known units (== threshold)
        let math = compute_payroll(&[row("404", 120, Some(1.5), false)], 40.0, false);
        close(math.total_units_known, 180.0);
        close(math.extra_unit_pay, 0.0);
        close(math.gross_pay.unwrap(), 1140.0);
        assert_eq!(math.status, "final");
    }

    #[test]
    fn above_threshold_pays_extra_units() {
        // 200 lengths × 1.5 = 300 known units → 120 extra × $3.80
        let math = compute_payroll(&[row("404", 200, Some(1.5), false)], 40.0, false);
        close(math.total_units_known, 300.0);
        close(math.extra_unit_pay, 120.0 * 3.80);
        close(math.gross_pay.unwrap(), 1140.0 + 120.0 * 3.80);
        assert_eq!(math.status, "final");
    }

    #[test]
    fn multiple_known_rows_sum_together() {
        // 100 × 1.5 + 40 × 2.0 = 150 + 80 = 230 → 50 extra
        let math = compute_payroll(
            &[
                row("404", 100, Some(1.5), false),
                row("722", 40, Some(2.0), false),
            ],
            40.0,
            false,
        );
        close(math.total_units_known, 230.0);
        close(math.extra_unit_pay, 50.0 * 3.80);
        close(math.gross_pay.unwrap(), 1140.0 + 50.0 * 3.80);
    }

    #[test]
    fn custom_rows_are_unknown_and_merged_per_model() {
        let math = compute_payroll(
            &[
                row("X1", 10, None, true),
                row("404", 100, Some(1.5), false),
                row("X1", 5, None, true),
            ],
            40.0,
            false,
        );
        assert_eq!(math.status, "unresolved");
        assert_eq!(math.gross_pay, None);
        close(math.extra_unit_pay, 0.0);
        close(math.total_units_known, 150.0);
        assert_eq!(math.unknown_details.len(), 1);
        assert_eq!(math.unknown_details[0].model, "X1");
        assert_eq!(math.unknown_details[0].quantity, 15);
        close(math.total_units_unknown, 15.0);
    }

    #[test]
    fn null_unit_value_is_unknown_even_when_not_custom() {
        let math = compute_payroll(&[row("X1", 10, None, false)], 40.0, false);
        assert_eq!(math.status, "unresolved");
        assert_eq!(math.unknown_details.len(), 1);
        assert_eq!(math.unknown_details[0].quantity, 10);
    }

    #[test]
    fn unresolved_equation_lists_known_units_and_unknown_parts() {
        let math = compute_payroll(
            &[
                row("X1", 15, None, true),
                row("404", 100, Some(1.5), false),
            ],
            40.0,
            false,
        );
        assert_eq!(math.pay_equation, "150 units + 15×X1 − 180 (base units)");
    }

    #[test]
    fn final_equation_shows_breakdown() {
        let math = compute_payroll(&[row("404", 200, Some(1.5), false)], 40.0, false);
        assert_eq!(
            math.pay_equation,
            "$1140.00 (base) + $456.00 (120 extra units × $3.80) = $1596.00"
        );
    }

    #[test]
    fn empty_week_pays_base_with_no_units() {
        let math = compute_payroll(&[], 40.0, false);
        close(math.total_units_known, 0.0);
        close(math.gross_pay.unwrap(), 1140.0);
        assert_eq!(math.status, "final");
    }

    // ---- Proration (base = $28.50/hr, threshold = 4.5 units/hr × hours) ----

    #[test]
    fn full_week_earnings_are_standard() {
        // 40 hr → base 1140, threshold 180.
        let math = compute_payroll(&[row("404", 200, Some(1.5), false)], 40.0, false);
        close(math.base_pay, 1140.0);
        close(math.unit_threshold, 180.0);
    }

    #[test]
    fn base_pay_prorates_with_hours_worked() {
        // User's example: absent 2 days → 24 hr. base = 28.50 × 24 = 684,
        // threshold = 4.5 × 24 = 108.
        let math = compute_payroll(&[row("404", 100, Some(1.0), false)], 24.0, false);
        close(math.base_pay, 684.0);
        close(math.unit_threshold, 108.0);
    }

    #[test]
    fn partial_week_prorates_base_and_threshold_down() {
        // 4 days = 32 hr. base = 28.50 × 32 = 912; threshold = 4.5 × 32 = 144.
        // 200 known units → 56 extra.
        let math = compute_payroll(&[row("404", 200, Some(1.0), false)], 32.0, false);
        close(math.base_pay, 912.0);
        close(math.unit_threshold, 144.0);
        close(math.total_units_known, 200.0);
        close(math.extra_unit_pay, 56.0 * 3.80);
        close(math.gross_pay.unwrap(), 912.0 + 56.0 * 3.80);
    }

    #[test]
    fn overtime_week_prorates_base_and_threshold_up() {
        // 48 hr. base = 28.50 × 48 = 1368; threshold = 4.5 × 48 = 216.
        // 250 known units → 34 extra.
        let math = compute_payroll(&[row("404", 250, Some(1.0), false)], 48.0, false);
        close(math.base_pay, 1368.0);
        close(math.unit_threshold, 216.0);
        close(math.extra_unit_pay, 34.0 * 3.80);
    }

    #[test]
    fn zero_hours_gives_zero_base_and_threshold_all_units_extra() {
        // No clock data → base 0, threshold 0, every known unit is extra.
        let math = compute_payroll(&[row("404", 100, Some(1.0), false)], 0.0, false);
        close(math.base_pay, 0.0);
        close(math.unit_threshold, 0.0);
        close(math.extra_unit_pay, 100.0 * 3.80);
    }

    #[test]
    fn standard_week_override_forces_full_week_pay() {
        // Admin overrides a short week to standard: prorate by 40 hr regardless
        // of clocked hours → base 1140, threshold 180. 200 units → 20 extra.
        let math = compute_payroll(&[row("404", 200, Some(1.0), false)], STANDARD_WEEK_HOURS, true);
        close(math.base_pay, 1140.0);
        close(math.unit_threshold, 180.0);
        close(math.extra_unit_pay, 20.0 * 3.80); // 200 - 180
        assert!(math.threshold_note.contains("admin override"));
    }

    #[test]
    fn hours_need_review_outside_band() {
        assert!(!hours_need_review(40.0));
        assert!(!hours_need_review(39.0));
        assert!(!hours_need_review(41.0));
        assert!(hours_need_review(38.9));
        assert!(hours_need_review(41.1));
        assert!(hours_need_review(0.0));
    }

    #[test]
    fn final_status_review_only_for_unreviewed_outside_band() {
        // Unreviewed partial week → review.
        assert_eq!(final_status("final", 32.0, false), "review");
        // Same week but admin reviewed → final.
        assert_eq!(final_status("final", 32.0, true), "final");
        // Normal week, unreviewed → final.
        assert_eq!(final_status("final", 40.0, false), "final");
        // Unknown rates always win.
        assert_eq!(final_status("unresolved", 40.0, true), "unresolved");
    }
}
