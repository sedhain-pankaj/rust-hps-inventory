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

/// Round to cents — keeps stored/sent money values free of IEEE-754 noise
/// (e.g. 209.9 - 180 = 29.900000000000006).
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Pure payroll math: splits cornice rows into known units and unknown-rate
/// details, then computes pay. The first 180 made units are included in base
/// pay; each unit above that earns the extra unit rate. Any custom or
/// unknown-rate row makes the week "unresolved" until an admin sets the rate.
fn compute_payroll(rows: &[CorniceRowForPay]) -> PayrollMath {
    const UNIT_THRESHOLD: f64 = 180.0;
    const BASE_PAY: f64 = 1140.0;
    const EXTRA_UNIT_RATE: f64 = 3.80;

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
            UNIT_THRESHOLD
        );
        (None, 0.0, eq, "unresolved".to_string())
    } else {
        let extra_units = round2((total_units_known - UNIT_THRESHOLD).max(0.0));
        let eup = round2(extra_units * EXTRA_UNIT_RATE);
        let gp = round2(BASE_PAY + eup);
        let eq = format!(
            "${:.2} (base) + ${:.2} ({:.0} extra units × $3.80) = ${:.2}",
            BASE_PAY, eup, extra_units, gp
        );
        (Some(gp), eup, eq, "final".to_string())
    };

    PayrollMath {
        total_units_known,
        total_units_unknown,
        unknown_details,
        unit_threshold: UNIT_THRESHOLD,
        threshold_note: "First 180 made units included in base pay".to_string(),
        base_pay: BASE_PAY,
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

    let week_end = req_week_start + chrono::Duration::days(6);

    // Total hours from clock events
    let clock_rows = sqlx::query(
        r#"
        SELECT * FROM time_clock_events
        WHERE employee_id = ? AND work_date >= ? AND work_date <= ?
        ORDER BY timestamp ASC, id ASC
        "#,
    )
    .bind(&employee.id)
    .bind(req_week_start.format("%Y-%m-%d").to_string())
    .bind(week_end.format("%Y-%m-%d").to_string())
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    let (total_seconds, ..) = seconds_from_event_rows(&clock_rows, false);
    let total_hours = total_seconds as f64 / 3600.0;

    // Cornice logs for the week
    let cornice_rows = sqlx::query(
        r#"
        SELECT model, lengths, unit_value, is_custom
        FROM cornice_logs
        WHERE employee_id = ? AND week_start = ?
        ORDER BY id ASC
        "#,
    )
    .bind(&employee.id)
    .bind(req_week_start.format("%Y-%m-%d").to_string())
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    let needs_review_hours = false;
    let math = compute_payroll(&cornice_rows_for_pay(&cornice_rows));

    // Persist payroll period
    let week_end_str = week_end.format("%Y-%m-%d").to_string();
    let gross_for_db = math.gross_pay.unwrap_or(0.0);
    let needs_review = needs_review_hours || math.status == "unresolved";

    sqlx::query(
        r#"
        INSERT INTO payroll_periods
            (employee_id, week_start, week_end, total_hours, total_units_known, unit_threshold,
             base_pay, extra_unit_pay, gross_pay, status, unknown_rate_equation, needs_admin_review, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            created_at = excluded.created_at
        "#,
    )
    .bind(&employee.id)
    .bind(req_week_start.format("%Y-%m-%d").to_string())
    .bind(&week_end_str)
        .bind(total_hours)
        .bind(math.total_units_known)
        .bind(math.unit_threshold)
        .bind(math.base_pay)
        .bind(math.extra_unit_pay)
        .bind(gross_for_db)
        .bind(&math.status)
        .bind(&math.pay_equation)
        .bind(needs_review as i64)
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    // Raise alert for unresolved or outside-band
    if math.status == "unresolved" {
        notification(
            &state.db,
            "red",
            "payroll_unresolved",
            &format!("{} has unknown-rate cornice units for week {}. Pay cannot be finalized.", employee.name, req_week_start.format("%Y-%m-%d")),
            "payroll_periods",
            None,
        )
        .await
        .ok();
    } else if needs_review_hours {
        notification(
            &state.db,
            "yellow",
            "payroll_proration",
            &format!("{} worked {:.1} hrs for week {} (outside 39-41 band). Prorated threshold: {:.0} units. Admin review needed.", employee.name, total_hours, req_week_start.format("%Y-%m-%d"), math.unit_threshold),
            "payroll_periods",
            None,
        )
        .await
        .ok();
    }

    Ok(PayrollWeekResponse {
        employee_id: employee.id,
        employee_name: employee.name,
        week_start: req_week_start.format("%Y-%m-%d").to_string(),
        week_end: week_end_str,
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
        status: math.status,
        needs_admin_review: needs_review,
    })
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
    let mut results = Vec::new();

    for employee in employees {
        let result = get_payroll_week_inner(
            &state.db,
            &employee.id,
            &employee.name,
            req_week_start,
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

// ==================== Helper functions for new commands ====================

async fn get_payroll_week_inner(
    db: &sqlx::SqlitePool,
    employee_id: &str,
    employee_name: &str,
    week_start: chrono::NaiveDate,
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

    let math = compute_payroll(&cornice_rows_for_pay(&cornice_rows));

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
        needs_admin_review: math.status == "unresolved",
        status: math.status,
    })
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

    // If overriding to standard (not accept_prorated), update the payroll_periods record
    if !input.accept_prorated {
        sqlx::query(
            r#"
            UPDATE payroll_periods
            SET unit_threshold = 180.0,
                status = 'review',
                unknown_rate_equation = 'Overridden to standard 40-hr / 180-unit week by admin.',
                needs_admin_review = 1
            WHERE employee_id = ? AND week_start = ?
            "#,
        )
        .bind(employee_id)
        .bind(week_start_str)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    } else {
        sqlx::query(
            r#"
            UPDATE payroll_periods
            SET status = 'final',
                needs_admin_review = 0
            WHERE employee_id = ? AND week_start = ?
            "#,
        )
        .bind(employee_id)
        .bind(week_start_str)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    }

    // Resolve the related alert
    sqlx::query(
        r#"
        UPDATE admin_notifications
        SET resolved = 1
        WHERE kind = 'payroll_proration'
          AND message LIKE ?
          AND resolved = 0
        "#,
    )
    .bind(format!("%{}%", employee_id))
    .execute(&state.db)
    .await
    .ok();

    // Recalculate and return
    get_payroll_week_inner(&state.db, employee_id, &employee.name, week_start).await
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
        let math = compute_payroll(&[row("404", 100, Some(1.5), false)]);
        close(math.total_units_known, 150.0);
        close(math.extra_unit_pay, 0.0);
        close(math.gross_pay.unwrap(), 1140.0);
        assert_eq!(math.status, "final");
        assert!(math.unknown_details.is_empty());
    }

    #[test]
    fn exactly_at_threshold_pays_base_only() {
        // 120 lengths × 1.5 = 180 known units (== threshold)
        let math = compute_payroll(&[row("404", 120, Some(1.5), false)]);
        close(math.total_units_known, 180.0);
        close(math.extra_unit_pay, 0.0);
        close(math.gross_pay.unwrap(), 1140.0);
        assert_eq!(math.status, "final");
    }

    #[test]
    fn above_threshold_pays_extra_units() {
        // 200 lengths × 1.5 = 300 known units → 120 extra × $3.80
        let math = compute_payroll(&[row("404", 200, Some(1.5), false)]);
        close(math.total_units_known, 300.0);
        close(math.extra_unit_pay, 120.0 * 3.80);
        close(math.gross_pay.unwrap(), 1140.0 + 120.0 * 3.80);
        assert_eq!(math.status, "final");
    }

    #[test]
    fn multiple_known_rows_sum_together() {
        // 100 × 1.5 + 40 × 2.0 = 150 + 80 = 230 → 50 extra
        let math = compute_payroll(&[
            row("404", 100, Some(1.5), false),
            row("722", 40, Some(2.0), false),
        ]);
        close(math.total_units_known, 230.0);
        close(math.extra_unit_pay, 50.0 * 3.80);
        close(math.gross_pay.unwrap(), 1140.0 + 50.0 * 3.80);
    }

    #[test]
    fn custom_rows_are_unknown_and_merged_per_model() {
        let math = compute_payroll(&[
            row("X1", 10, None, true),
            row("404", 100, Some(1.5), false),
            row("X1", 5, None, true),
        ]);
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
        let math = compute_payroll(&[row("X1", 10, None, false)]);
        assert_eq!(math.status, "unresolved");
        assert_eq!(math.unknown_details.len(), 1);
        assert_eq!(math.unknown_details[0].quantity, 10);
    }

    #[test]
    fn unresolved_equation_lists_known_units_and_unknown_parts() {
        let math = compute_payroll(&[
            row("X1", 15, None, true),
            row("404", 100, Some(1.5), false),
        ]);
        assert_eq!(math.pay_equation, "150 units + 15×X1 − 180 (base units)");
    }

    #[test]
    fn final_equation_shows_breakdown() {
        let math = compute_payroll(&[row("404", 200, Some(1.5), false)]);
        assert_eq!(
            math.pay_equation,
            "$1140.00 (base) + $456.00 (120 extra units × $3.80) = $1596.00"
        );
    }

    #[test]
    fn empty_week_pays_base_with_no_units() {
        let math = compute_payroll(&[]);
        close(math.total_units_known, 0.0);
        close(math.gross_pay.unwrap(), 1140.0);
        assert_eq!(math.status, "final");
    }
}
