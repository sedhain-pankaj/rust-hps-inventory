use chrono::{Local, NaiveDate, NaiveDateTime, Timelike};
use sqlx::Row;
use tauri::State;

use crate::{
    db::{
        employee_by_id, format_seconds, list_employees, notification, parse_date_or_today,
        today_string, week_start_for, AppState,
    },
    models::*,
};

use super::rates::find_rate_for_model;
use super::{to_string, CommandResult};

#[tauri::command]
pub async fn record_clock_event(
    state: State<'_, AppState>,
    request: ClockRequest,
) -> CommandResult<ClockEvent> {
    refresh_attendance_issues(&state.db).await?;
    let employee = employee_by_id(&state.db, &request.employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee not found.".to_string())?;
    if !employee.active {
        return Err(format!("{} is inactive.", employee.name));
    }
    if request.action != "clock_in" && request.action != "clock_out" {
        return Err("Choose clock in or clock out.".to_string());
    }

    let work_date = today_string();
    let now_local = Local::now();
    let now = now_local.format("%Y-%m-%dT%H:%M:%S").to_string();
    let last_action: Option<String> = sqlx::query(
        r#"
        SELECT action FROM time_clock_events
        WHERE employee_id = ? AND work_date = ?
        ORDER BY timestamp DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(&request.employee_id)
    .bind(&work_date)
    .fetch_optional(&state.db)
    .await
    .map_err(to_string)?
    .map(|row| row.get("action"));

    let has_clock_in = sqlx::query(
        "SELECT 1 FROM time_clock_events WHERE employee_id = ? AND work_date = ? AND action = 'clock_in' LIMIT 1",
    )
    .bind(&request.employee_id)
    .bind(&work_date)
    .fetch_optional(&state.db)
    .await
    .map_err(to_string)?
    .is_some();

    let mut notes = Vec::new();
    if request.action == "clock_out" && !has_clock_in {
        notes.push("Clock-in missing; admin review required.".to_string());
    } else if request.action == "clock_in" && last_action.as_deref() == Some("clock_in") {
        notes.push("Employee clocked in twice without a clock-out.".to_string());
    } else if request.action == "clock_out" && last_action.as_deref() == Some("clock_out") {
        notes.push("Employee clocked out twice.".to_string());
    }

    let hour = now_local.hour();
    if request.action == "clock_in" && !(5..=9).contains(&hour) {
        notes.push("Clock-in is outside the usual 5-9am window.".to_string());
    }
    if request.action == "clock_out" && hour < 13 {
        notes.push("Clock-out is before the usual after-1pm window.".to_string());
    }

    notes.sort();
    notes.dedup();
    let needs_review = !notes.is_empty();
    let note = notes.join(" ");

    let result = sqlx::query(
        r#"
        INSERT INTO time_clock_events
            (employee_id, work_date, action, timestamp, source, needs_admin_review, note)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&request.employee_id)
    .bind(&work_date)
    .bind(&request.action)
    .bind(&now)
    .bind(request.source.trim())
    .bind(needs_review as i64)
    .bind(&note)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    let id = result.last_insert_rowid();
    if needs_review {
        notification(
            &state.db,
            "red",
            "attendance",
            &format!("{}: {}", employee.name, note),
            "time_clock_events",
            Some(id),
        )
        .await
        .map_err(to_string)?;
    }

    clock_event_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn list_clock_events(
    state: State<'_, AppState>,
    date: Option<String>,
) -> CommandResult<Vec<ClockEvent>> {
    let work_date = date.unwrap_or_else(today_string);
    let rows = sqlx::query(
        r#"
        SELECT t.*, e.name AS employee_name
        FROM time_clock_events t
        JOIN employees e ON e.id = t.employee_id
        WHERE t.work_date = ?
        ORDER BY t.timestamp DESC, t.id DESC
        "#,
    )
    .bind(work_date)
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(clock_event_from_row).collect())
}

#[tauri::command]
pub async fn get_clock_status(
    state: State<'_, AppState>,
    employee_id: String,
) -> CommandResult<ClockStatus> {
    let today = today_string();
    let last: Option<String> = sqlx::query_scalar(
        r#"
        SELECT action FROM time_clock_events
        WHERE employee_id = ? AND work_date = ?
        ORDER BY timestamp DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(&employee_id)
    .bind(&today)
    .fetch_optional(&state.db)
    .await
    .map_err(to_string)?;

    let today_state = match last.as_deref() {
        Some("clock_in") => "in",
        Some("clock_out") => "out",
        _ => "none",
    }
    .to_string();

    let yesterday = (Local::now().date_naive() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    let missed: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1 FROM time_clock_events t
        WHERE t.employee_id = ? AND t.work_date = ? AND t.action = 'clock_in'
          AND NOT EXISTS (
              SELECT 1 FROM time_clock_events o
              WHERE o.employee_id = t.employee_id
                AND o.work_date = t.work_date
                AND o.action = 'clock_out'
                AND o.timestamp > t.timestamp
          )
        LIMIT 1
        "#,
    )
    .bind(&employee_id)
    .bind(&yesterday)
    .fetch_optional(&state.db)
    .await
    .map_err(to_string)?;

    Ok(ClockStatus {
        today_state,
        missed_yesterday_clock_out: missed.is_some(),
    })
}

#[tauri::command]
pub async fn attendance_today(state: State<'_, AppState>) -> CommandResult<Vec<AttendanceSummary>> {
    attendance_for_date(&state.db, Local::now().date_naive()).await
}

#[tauri::command]
pub async fn attendance_for_week(
    state: State<'_, AppState>,
    week_start: Option<String>,
) -> CommandResult<Vec<AttendanceSummary>> {
    refresh_attendance_issues(&state.db).await?;
    let start = parse_date_or_today(week_start);
    let end = start + chrono::Duration::days(6);
    let employees = list_employees(&state.db, true).await.map_err(to_string)?;
    let mut summaries = Vec::new();

    for employee in employees {
        let rows = sqlx::query(
            r#"
            SELECT * FROM time_clock_events
            WHERE employee_id = ? AND work_date >= ? AND work_date <= ?
            ORDER BY timestamp ASC, id ASC
            "#,
        )
        .bind(&employee.id)
        .bind(start.format("%Y-%m-%d").to_string())
        .bind(end.format("%Y-%m-%d").to_string())
        .fetch_all(&state.db)
        .await
        .map_err(to_string)?;

        let (seconds, needs_review, note) = seconds_from_event_rows(&rows, false);
        summaries.push(AttendanceSummary {
            employee_id: employee.id,
            employee_name: employee.name,
            work_date: start.format("%Y-%m-%d").to_string(),
            hours: format_seconds(seconds),
            seconds,
            status: "Week total".to_string(),
            needs_admin_review: needs_review,
            note,
        });
    }

    Ok(summaries)
}

#[tauri::command]
pub async fn add_cornice_log(
    state: State<'_, AppState>,
    input: CorniceLogInput,
) -> CommandResult<CorniceLog> {
    if input.employee_id.trim().is_empty() || input.model.trim().is_empty() {
        return Err("Employee and model are required.".to_string());
    }
    if input.lengths <= 0 {
        return Err("Lengths must be greater than zero.".to_string());
    }

    let date = parse_date_or_today(input.log_date);
    let log_date = date.format("%Y-%m-%d").to_string();
    let week_start = week_start_for(date).format("%Y-%m-%d").to_string();
    let rate = find_rate_for_model(&state.db, input.model.trim())
        .await
        .map_err(to_string)?;

    let (series, unit, unit_value, is_custom) = match rate {
        Some(rate) => (rate.series, rate.unit.clone(), crate::db::unit_value(&rate.unit), false),
        None => (input.series.trim().to_string(), String::new(), None, true),
    };
    let total_units = (unit_value.unwrap_or(0.0) * input.lengths as f64 * 100.0).round() / 100.0;
    let needs_review = is_custom || unit_value.is_none();

    let result = sqlx::query(
        r#"
        INSERT INTO cornice_logs
            (employee_id, log_date, week_start, series, model, lengths, unit,
             unit_value, total_units, is_custom, needs_admin_review, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(input.employee_id.trim())
    .bind(&log_date)
    .bind(&week_start)
    .bind(series)
    .bind(input.model.trim())
    .bind(input.lengths)
    .bind(unit)
    .bind(unit_value)
    .bind(total_units)
    .bind(is_custom as i64)
    .bind(needs_review as i64)
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    let id = result.last_insert_rowid();
    if needs_review {
        notification(
            &state.db,
            "red",
            "cornice_log",
            &format!(
                "Unknown or custom cornice model {} was logged.",
                input.model.trim()
            ),
            "cornice_logs",
            Some(id),
        )
        .await
        .map_err(to_string)?;
    }

    cornice_log_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn list_cornice_logs(
    state: State<'_, AppState>,
    employee_id: Option<String>,
    date: Option<String>,
    week_start: Option<String>,
) -> CommandResult<Vec<CorniceLog>> {
    let rows = sqlx::query(
        r#"
        SELECT c.*, e.name AS employee_name
        FROM cornice_logs c
        JOIN employees e ON e.id = c.employee_id
        WHERE (? IS NULL OR c.employee_id = ?)
          AND (? IS NULL OR c.log_date = ?)
          AND (? IS NULL OR c.week_start = ?)
        ORDER BY c.log_date DESC, c.id DESC
        "#,
    )
    .bind(employee_id.as_deref())
    .bind(employee_id.as_deref())
    .bind(date.as_deref())
    .bind(date.as_deref())
    .bind(week_start.as_deref())
    .bind(week_start.as_deref())
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    let mut logs = Vec::with_capacity(rows.len());
    for row in rows {
        logs.push(cornice_log_from_row(&state.db, row).await?);
    }
    Ok(logs)
}

#[tauri::command]
pub async fn update_cornice_log(
    state: State<'_, AppState>,
    input: CorniceLogUpdateInput,
) -> CommandResult<CorniceLog> {
    let existing = cornice_log_by_id(&state.db, input.id).await?;
    if existing.employee_id != input.actor_id.trim() {
        return Err("You can only edit your own log entries.".to_string());
    }
    if input.model.trim().is_empty() {
        return Err("Model is required.".to_string());
    }
    if input.lengths <= 0 {
        return Err("Lengths must be greater than zero.".to_string());
    }

    let rate = find_rate_for_model(&state.db, input.model.trim())
        .await
        .map_err(to_string)?;
    let (series, unit, unit_value, is_custom) = match rate {
        Some(rate) => (rate.series, rate.unit.clone(), crate::db::unit_value(&rate.unit), false),
        None => (input.series.trim().to_string(), String::new(), None, true),
    };
    let total_units = (unit_value.unwrap_or(0.0) * input.lengths as f64 * 100.0).round() / 100.0;
    let now = crate::db::now_string();

    let mut prev = serde_json::Map::new();
    if existing.model != input.model.trim() {
        prev.insert("model".to_string(), serde_json::Value::String(existing.model.clone()));
    }
    if existing.lengths != input.lengths {
        prev.insert("lengths".to_string(), serde_json::Value::from(existing.lengths));
    }
    let changed = !prev.is_empty();
    let same_day = existing.log_date == today_string();
    let needs_review = if same_day {
        is_custom || unit_value.is_none()
    } else {
        changed || is_custom || unit_value.is_none()
    };
    let prev_values = if !same_day && changed {
        Some(serde_json::Value::Object(prev).to_string())
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE cornice_logs
        SET series = ?, model = ?, lengths = ?, unit = ?, unit_value = ?, total_units = ?,
            is_custom = ?, needs_admin_review = ?, prev_values = ?, amended_at = ?, amended_by = ?
        WHERE id = ?
        "#,
    )
    .bind(series)
    .bind(input.model.trim())
    .bind(input.lengths)
    .bind(unit)
    .bind(unit_value)
    .bind(total_units)
    .bind(is_custom as i64)
    .bind(needs_review as i64)
    .bind(&prev_values)
    .bind(&now)
    .bind(input.actor_id.trim())
    .bind(input.id)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    if !same_day && needs_review {
        notification(
            &state.db,
            "yellow",
            "cornice_log_edit",
            &format!(
                "{} edited a past entry ({} {}). Pending approval.",
                existing.employee_name, existing.log_date, existing.model
            ),
            "cornice_logs",
            Some(input.id),
        )
        .await
        .map_err(to_string)?;
    }

    cornice_log_by_id(&state.db, input.id).await
}

#[tauri::command]
pub async fn delete_cornice_log(
    state: State<'_, AppState>,
    id: i64,
    actor_id: String,
) -> CommandResult<()> {
    let existing = cornice_log_by_id(&state.db, id).await?;
    if existing.employee_id != actor_id.trim() {
        return Err("You can only delete your own log entries.".to_string());
    }
    let now = crate::db::now_string();
    if existing.log_date == today_string() {
        sqlx::query("DELETE FROM cornice_logs WHERE id = ?")
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        return Ok(());
    }
    let prev = serde_json::json!({
        "deleted": true,
        "model": existing.model,
        "lengths": existing.lengths,
        "total_units": existing.total_units,
    });
    sqlx::query(
        r#"
        UPDATE cornice_logs
        SET needs_admin_review = 1, prev_values = ?, amended_at = ?, amended_by = ?
        WHERE id = ?
        "#,
    )
    .bind(prev.to_string())
    .bind(&now)
    .bind(actor_id.trim())
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(to_string)?;
    notification(
        &state.db,
        "yellow",
        "cornice_log_edit",
        &format!(
            "{} deleted a past entry ({} {}). Pending approval.",
            existing.employee_name, existing.log_date, existing.model
        ),
        "cornice_logs",
        Some(id),
    )
    .await
    .map_err(to_string)?;
    Ok(())
}

#[tauri::command]
pub async fn approve_cornice_log(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    let prev: Option<String> = sqlx::query_scalar("SELECT prev_values FROM cornice_logs WHERE id = ?")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(to_string)?;
    let staged_delete = prev
        .as_deref()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .and_then(|value| value.get("deleted").and_then(|flag| flag.as_bool()))
        .unwrap_or(false);

    if staged_delete {
        sqlx::query("DELETE FROM cornice_logs WHERE id = ?")
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
    } else {
        sqlx::query("UPDATE cornice_logs SET prev_values = NULL, needs_admin_review = 0 WHERE id = ?")
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
    }

    sqlx::query(
        r#"
        UPDATE admin_notifications SET resolved = 1
        WHERE kind = 'cornice_log_edit' AND entity_table = 'cornice_logs' AND entity_id = ? AND resolved = 0
        "#,
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(to_string)?;
    Ok(())
}

#[tauri::command]
pub async fn add_production_log(
    state: State<'_, AppState>,
    input: ProductionLogInput,
) -> CommandResult<ProductionLog> {
    let date = parse_date_or_today(input.log_date);
    let result = sqlx::query(
        r#"
        INSERT INTO production_logs
            (employee_id, log_date, item, quantity, notes, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(input.employee_id.trim())
    .bind(date.format("%Y-%m-%d").to_string())
    .bind(input.item.trim())
    .bind(input.quantity)
    .bind(input.notes.trim())
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    production_log_by_id(&state.db, result.last_insert_rowid()).await
}

#[tauri::command]
pub async fn list_production_logs(
    state: State<'_, AppState>,
    employee_id: Option<String>,
    date: Option<String>,
) -> CommandResult<Vec<ProductionLog>> {
    let rows = sqlx::query(
        r#"
        SELECT p.*, e.name AS employee_name
        FROM production_logs p
        JOIN employees e ON e.id = p.employee_id
        WHERE (? IS NULL OR p.employee_id = ?)
          AND (? IS NULL OR p.log_date = ?)
        ORDER BY p.log_date DESC, p.id DESC
        "#,
    )
    .bind(employee_id.as_deref())
    .bind(employee_id.as_deref())
    .bind(date.as_deref())
    .bind(date.as_deref())
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(production_log_from_row).collect())
}

#[tauri::command]
pub async fn add_overstock(
    state: State<'_, AppState>,
    input: OverstockInput,
) -> CommandResult<OverstockItem> {
    let result = sqlx::query(
        r#"
        INSERT INTO overstock_locations
            (model, quantity, aisle, notes, updated_by, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(input.model.trim())
    .bind(input.quantity)
    .bind(input.aisle.trim())
    .bind(input.notes.trim())
    .bind(input.employee_id.trim())
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    overstock_by_id(&state.db, result.last_insert_rowid()).await
}

#[tauri::command]
pub async fn list_overstock(state: State<'_, AppState>) -> CommandResult<Vec<OverstockItem>> {
    let rows = sqlx::query(
        r#"
        SELECT id, model, quantity, aisle, notes, updated_by, updated_at
        FROM overstock_locations
        ORDER BY updated_at DESC, id DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(overstock_from_row).collect())
}

#[tauri::command]
pub async fn add_delivery(
    state: State<'_, AppState>,
    input: DeliveryInput,
) -> CommandResult<Delivery> {
    let date = parse_date_or_today(input.delivery_date);
    let result = sqlx::query(
        r#"
        INSERT INTO deliveries
            (driver_id, delivery_date, address, items, notes, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(input.driver_id.trim())
    .bind(date.format("%Y-%m-%d").to_string())
    .bind(input.address.trim())
    .bind(input.items.trim())
    .bind(input.notes.trim())
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    delivery_by_id(&state.db, result.last_insert_rowid()).await
}

#[tauri::command]
pub async fn list_deliveries(
    state: State<'_, AppState>,
    date: Option<String>,
) -> CommandResult<Vec<Delivery>> {
    let rows = sqlx::query(
        r#"
        SELECT d.*, e.name AS driver_name
        FROM deliveries d
        JOIN employees e ON e.id = d.driver_id
        WHERE (? IS NULL OR d.delivery_date = ?)
        ORDER BY d.delivery_date DESC, d.id DESC
        "#,
    )
    .bind(date.as_deref())
    .bind(date.as_deref())
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(delivery_from_row).collect())
}

// ==================== Clock Event Edit (Audit Trail) ====================

#[tauri::command]
pub async fn edit_clock_event(
    state: State<'_, AppState>,
    input: EditClockEventInput,
    edited_by: String,
) -> CommandResult<ClockEvent> {
    let field = input.field_name.trim();
    let new_val = input.new_value.trim().to_string();

    // Validate field name
    if !["timestamp", "action", "work_date", "source", "note"].contains(&field) {
        return Err(format!("Cannot edit field '{}'. Allowed: timestamp, action, work_date, source, note", field));
    }

    // Get old value
    let old_row = sqlx::query(
        r#"
        SELECT t.*, e.name AS employee_name
        FROM time_clock_events t
        JOIN employees e ON e.id = t.employee_id
        WHERE t.id = ?
        "#,
    )
    .bind(input.event_id)
    .fetch_one(&state.db)
    .await
    .map_err(to_string)?;

    let old_value: String = old_row.get(field);

    // Validate action if editing that field
    if field == "action" && !["clock_in", "clock_out"].contains(&new_val.as_str()) {
        return Err("Action must be 'clock_in' or 'clock_out'.".to_string());
    }

    // Perform the update
    let assignments = format!("{} = ?", field);
    sqlx::query(format!("UPDATE time_clock_events SET {assignments}, needs_admin_review = 1 WHERE id = ?").as_str())
        .bind(&new_val)
        .bind(input.event_id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;

    // Log the edit
    sqlx::query(
        r#"
        INSERT INTO clock_event_edits (event_id, edited_by, field_name, old_value, new_value, reason, edited_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(input.event_id)
    .bind(edited_by.trim())
    .bind(field)
    .bind(&old_value)
    .bind(&new_val)
    .bind(input.reason.trim())
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    // Raise alert for payroll-affecting edit
    let employee_name: String = old_row.get("employee_name");
    notification(
        &state.db,
        "yellow",
        "clock_edit",
        &format!("{} edited {} clock event field '{}' from '{}' to '{}'", edited_by, employee_name, field, old_value, new_val),
        "time_clock_events",
        Some(input.event_id),
    )
    .await
    .map_err(to_string)?;

    // Return updated event
    let row = sqlx::query(
        r#"
        SELECT t.*, e.name AS employee_name
        FROM time_clock_events t
        JOIN employees e ON e.id = t.employee_id
        WHERE t.id = ?
        "#,
    )
    .bind(input.event_id)
    .fetch_one(&state.db)
    .await
    .map_err(to_string)?;

    Ok(clock_event_from_row(row))
}

#[tauri::command]
pub async fn list_clock_event_edits(
    state: State<'_, AppState>,
    event_id: Option<i64>,
) -> CommandResult<Vec<ClockEventEdit>> {
    let mut sql = String::from(
        r#"
        SELECT ce.*, e.name AS editor_name
        FROM clock_event_edits ce
        JOIN employees e ON e.id = ce.edited_by
        "#,
    );

    if let Some(eid) = event_id {
        sql.push_str(" WHERE ce.event_id = ?");
        let rows = sqlx::query(&sql)
            .bind(eid)
            .fetch_all(&state.db)
            .await
            .map_err(to_string)?;
        let mut edits = Vec::new();
        for row in rows {
            edits.push(ClockEventEdit {
                id: row.get("id"),
                event_id: row.get("event_id"),
                edited_by: row.get("editor_name"),
                field_name: row.get("field_name"),
                old_value: row.get("old_value"),
                new_value: row.get("new_value"),
                reason: row.get("reason"),
                edited_at: row.get("edited_at"),
            });
        }
        return Ok(edits);
    }

    sql.push_str(" ORDER BY ce.edited_at DESC, ce.id DESC");
    let rows = sqlx::query(&sql).fetch_all(&state.db).await.map_err(to_string)?;
    let mut edits = Vec::new();
    for row in rows {
        edits.push(ClockEventEdit {
            id: row.get("id"),
            event_id: row.get("event_id"),
            edited_by: row.get("editor_name"),
            field_name: row.get("field_name"),
            old_value: row.get("old_value"),
            new_value: row.get("new_value"),
            reason: row.get("reason"),
            edited_at: row.get("edited_at"),
        });
    }
    Ok(edits)
}

async fn clock_event_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<ClockEvent> {
    let row = sqlx::query(
        r#"
        SELECT t.*, e.name AS employee_name
        FROM time_clock_events t
        JOIN employees e ON e.id = t.employee_id
        WHERE t.id = ?
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;

    Ok(clock_event_from_row(row))
}

fn clock_event_from_row(row: sqlx::sqlite::SqliteRow) -> ClockEvent {
    ClockEvent {
        id: row.get("id"),
        employee_id: row.get("employee_id"),
        employee_name: row.get("employee_name"),
        work_date: row.get("work_date"),
        action: row.get("action"),
        timestamp: row.get("timestamp"),
        source: row.get("source"),
        needs_admin_review: row.get::<i64, _>("needs_admin_review") != 0,
        note: row.get("note"),
    }
}

async fn attendance_for_date(
    db: &sqlx::SqlitePool,
    date: NaiveDate,
) -> CommandResult<Vec<AttendanceSummary>> {
    refresh_attendance_issues(db).await?;
    let work_date = date.format("%Y-%m-%d").to_string();
    let employees = list_employees(db, true).await.map_err(to_string)?;
    let mut output = Vec::new();
    for employee in employees {
        let rows = sqlx::query(
            r#"
            SELECT * FROM time_clock_events
            WHERE employee_id = ? AND work_date = ?
            ORDER BY timestamp ASC, id ASC
            "#,
        )
        .bind(&employee.id)
        .bind(&work_date)
        .fetch_all(db)
        .await
        .map_err(to_string)?;
        if rows.is_empty() {
            continue;
        }
        let last_action = rows
            .last()
            .map(|row| row.get::<String, _>("action"))
            .unwrap_or_default();
        let (seconds, needs_review, note) =
            seconds_from_event_rows(&rows, date == Local::now().date_naive());
        output.push(AttendanceSummary {
            employee_id: employee.id,
            employee_name: employee.name,
            work_date: work_date.clone(),
            hours: format_seconds(seconds),
            seconds,
            status: if last_action == "clock_in" {
                "Clocked in".to_string()
            } else {
                "Clocked out".to_string()
            },
            needs_admin_review: needs_review,
            note,
        });
    }
    Ok(output)
}

pub(crate) fn seconds_from_event_rows(
    rows: &[sqlx::sqlite::SqliteRow],
    include_open_until_now: bool,
) -> (i64, bool, String) {
    let mut seconds = 0_i64;
    let mut open_start: Option<NaiveDateTime> = None;
    let mut needs_review = false;
    let mut notes = Vec::new();

    for row in rows {
        let action: String = row.get("action");
        let timestamp: String = row.get("timestamp");
        if row.get::<i64, _>("needs_admin_review") != 0 {
            needs_review = true;
            let note: String = row.get("note");
            if !note.is_empty() {
                notes.push(note);
            }
        }
        let parsed = parse_timestamp(&timestamp);
        match (action.as_str(), parsed) {
            ("clock_in", Some(time)) => {
                if open_start.is_some() {
                    needs_review = true;
                    notes.push("Repeated clock-in.".to_string());
                }
                open_start = Some(time);
            }
            ("clock_out", Some(time)) => {
                if let Some(start) = open_start.take() {
                    if time > start {
                        seconds += (time - start).num_seconds();
                    }
                } else {
                    needs_review = true;
                    notes.push("Clock-in missing.".to_string());
                }
            }
            _ => {}
        }
    }

    if let Some(start) = open_start {
        if include_open_until_now {
            let now = Local::now().naive_local();
            if now > start {
                seconds += (now - start).num_seconds();
            }
        } else {
            needs_review = true;
            notes.push("Clock-out missing.".to_string());
        }
    }

    notes.sort();
    notes.dedup();
    (seconds, needs_review, notes.join(" "))
}

pub(crate) async fn refresh_attendance_issues(db: &sqlx::SqlitePool) -> CommandResult<()> {
    let today = today_string();
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.employee_id, t.work_date, e.name AS employee_name
        FROM time_clock_events t
        JOIN employees e ON e.id = t.employee_id
        WHERE t.action = 'clock_in'
          AND t.work_date < ?
          AND NOT EXISTS (
              SELECT 1 FROM time_clock_events out
              WHERE out.employee_id = t.employee_id
                AND out.work_date = t.work_date
                AND out.timestamp > t.timestamp
                AND out.action = 'clock_out'
          )
        "#,
    )
    .bind(today)
    .fetch_all(db)
    .await
    .map_err(to_string)?;

    for row in rows {
        let id: i64 = row.get("id");
        let existing = sqlx::query(
            "SELECT 1 FROM admin_notifications WHERE kind = 'missing_clock_out' AND entity_table = 'time_clock_events' AND entity_id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(to_string)?
        .is_some();
        if !existing {
            let employee_name: String = row.get("employee_name");
            let work_date: String = row.get("work_date");
            sqlx::query(
                "UPDATE time_clock_events SET needs_admin_review = 1, note = 'Clock-out missing; admin review required.' WHERE id = ?",
            )
            .bind(id)
            .execute(db)
            .await
            .map_err(to_string)?;
            notification(
                db,
                "red",
                "missing_clock_out",
                &format!("{employee_name}: clock-out missing for {work_date}."),
                "time_clock_events",
                Some(id),
            )
            .await
            .map_err(to_string)?;
        }
    }

    Ok(())
}

async fn cornice_log_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<CorniceLog> {
    let row = sqlx::query(
        r#"
        SELECT c.*, e.name AS employee_name
        FROM cornice_logs c
        JOIN employees e ON e.id = c.employee_id
        WHERE c.id = ?
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    cornice_log_from_row(db, row).await
}

async fn cornice_log_from_row(
    db: &sqlx::SqlitePool,
    row: sqlx::sqlite::SqliteRow,
) -> CommandResult<CorniceLog> {
    let employee_id: String = row.get("employee_id");
    let week_start: String = row.get("week_start");
    let weekly_units = sqlx::query(
        "SELECT COALESCE(SUM(total_units), 0) AS total FROM cornice_logs WHERE employee_id = ? AND week_start = ?",
    )
    .bind(&employee_id)
    .bind(&week_start)
    .fetch_one(db)
    .await
    .map_err(to_string)?
    .get::<f64, _>("total");

    Ok(CorniceLog {
        id: row.get("id"),
        employee_id,
        employee_name: row.get("employee_name"),
        log_date: row.get("log_date"),
        week_start,
        series: row.get("series"),
        model: row.get("model"),
        lengths: row.get("lengths"),
        unit: row.get("unit"),
        unit_value: row.get("unit_value"),
        total_units: row.get("total_units"),
        weekly_units,
        is_custom: row.get::<i64, _>("is_custom") != 0,
        needs_admin_review: row.get::<i64, _>("needs_admin_review") != 0,
        prev_values: row.try_get::<Option<String>, _>("prev_values").unwrap_or(None),
        amended_at: row.try_get::<Option<String>, _>("amended_at").unwrap_or(None),
        amended_by: row.try_get::<Option<String>, _>("amended_by").unwrap_or(None),
    })
}

async fn production_log_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<ProductionLog> {
    let row = sqlx::query(
        r#"
        SELECT p.*, e.name AS employee_name
        FROM production_logs p
        JOIN employees e ON e.id = p.employee_id
        WHERE p.id = ?
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;

    Ok(production_log_from_row(row))
}

fn production_log_from_row(row: sqlx::sqlite::SqliteRow) -> ProductionLog {
    ProductionLog {
        id: row.get("id"),
        employee_id: row.get("employee_id"),
        employee_name: row.get("employee_name"),
        log_date: row.get("log_date"),
        item: row.get("item"),
        quantity: row.get("quantity"),
        notes: row.get("notes"),
    }
}

async fn overstock_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<OverstockItem> {
    let row = sqlx::query(
        "SELECT id, model, quantity, aisle, notes, updated_by, updated_at FROM overstock_locations WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(overstock_from_row(row))
}

fn overstock_from_row(row: sqlx::sqlite::SqliteRow) -> OverstockItem {
    OverstockItem {
        id: row.get("id"),
        model: row.get("model"),
        quantity: row.get("quantity"),
        aisle: row.get("aisle"),
        notes: row.get("notes"),
        updated_by: row.get("updated_by"),
        updated_at: row.get("updated_at"),
    }
}

async fn delivery_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<Delivery> {
    let row = sqlx::query(
        r#"
        SELECT d.*, e.name AS driver_name
        FROM deliveries d
        JOIN employees e ON e.id = d.driver_id
        WHERE d.id = ?
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(delivery_from_row(row))
}

fn delivery_from_row(row: sqlx::sqlite::SqliteRow) -> Delivery {
    Delivery {
        id: row.get("id"),
        driver_id: row.get("driver_id"),
        driver_name: row.get("driver_name"),
        delivery_date: row.get("delivery_date"),
        address: row.get("address"),
        items: row.get("items"),
        notes: row.get("notes"),
    }
}

fn parse_timestamp(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok()
}

