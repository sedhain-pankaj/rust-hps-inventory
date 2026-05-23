use chrono::{Local, NaiveDate, NaiveDateTime};
use sqlx::Row;
use tauri::State;

use crate::{
    db::{
        employee_by_id, format_seconds, hash_password, list_employees, notification,
        parse_date_or_today, today_string, week_start_for, AppState,
    },
    fingerprint,
    models::*,
};

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> CommandResult<AppStatus> {
    let helper = fingerprint::find_helper_binary(&state.paths);
    Ok(AppStatus {
        database_path: state.paths.db_path.to_string_lossy().to_string(),
        fingerprint_helper_found: helper.is_some(),
        fingerprint_helper_path: helper.map(|path| path.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub async fn list_staff(state: State<'_, AppState>, include_inactive: bool) -> CommandResult<Vec<Employee>> {
    list_employees(&state.db, include_inactive)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub async fn save_employee(state: State<'_, AppState>, input: EmployeeInput) -> CommandResult<Employee> {
    if input.id.trim().is_empty() {
        return Err("Employee ID is required.".to_string());
    }
    if input.name.trim().is_empty() {
        return Err("Employee name is required.".to_string());
    }

    let now = crate::db::now_string();
    let password_hash = input
        .password
        .as_ref()
        .map(|password| password.trim())
        .filter(|password| !password.is_empty())
        .map(hash_password);

    sqlx::query(
        r#"
        INSERT INTO employees
            (id, name, finger, active, is_admin, password_hash, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            finger = excluded.finger,
            active = excluded.active,
            is_admin = excluded.is_admin,
            password_hash = COALESCE(excluded.password_hash, employees.password_hash),
            updated_at = excluded.updated_at
        "#,
    )
    .bind(input.id.trim())
    .bind(input.name.trim())
    .bind(input.finger.trim())
    .bind(input.active as i64)
    .bind(input.is_admin as i64)
    .bind(password_hash)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    let mut permissions = input.permissions;
    if input.is_admin {
        for permission in [
            "clock",
            "cornice_log",
            "production_log",
            "overstock",
            "deliveries",
            "cornice_rates_view",
            "daily_production_all",
        ] {
            if !permissions.iter().any(|item| item == permission) {
                permissions.push(permission.to_string());
            }
        }
    }

    sqlx::query("DELETE FROM employee_permissions WHERE employee_id = ?")
        .bind(input.id.trim())
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    for permission in permissions {
        sqlx::query(
            "INSERT OR IGNORE INTO employee_permissions (employee_id, permission) VALUES (?, ?)",
        )
        .bind(input.id.trim())
        .bind(permission)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    }

    employee_by_id(&state.db, input.id.trim())
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee was saved but could not be reloaded.".to_string())
}

#[tauri::command]
pub async fn authenticate_password(
    state: State<'_, AppState>,
    employee_id: Option<String>,
    password: String,
    require_admin: bool,
) -> CommandResult<AuthResponse> {
    let password_hash = hash_password(password.trim());
    let candidates = if let Some(employee_id) = employee_id.filter(|id| !id.trim().is_empty()) {
        sqlx::query("SELECT id, password_hash, is_admin, active FROM employees WHERE id = ?")
            .bind(employee_id.trim())
            .fetch_all(&state.db)
            .await
            .map_err(to_string)?
    } else {
        sqlx::query("SELECT id, password_hash, is_admin, active FROM employees WHERE active = 1")
            .fetch_all(&state.db)
            .await
            .map_err(to_string)?
    };

    for row in candidates {
        let active = row.get::<i64, _>("active") != 0;
        let is_admin = row.get::<i64, _>("is_admin") != 0;
        let stored: Option<String> = row.get("password_hash");
        if !active || (require_admin && !is_admin) {
            continue;
        }
        if stored.as_deref() == Some(password_hash.as_str()) {
            let id: String = row.get("id");
            let employee = employee_by_id(&state.db, &id)
                .await
                .map_err(to_string)?
                .ok_or_else(|| "Employee no longer exists.".to_string())?;
            return Ok(AuthResponse {
                employee,
                source: "password".to_string(),
            });
        }
    }

    Err("Password was not accepted.".to_string())
}

#[tauri::command]
pub async fn authenticate_fingerprint(
    state: State<'_, AppState>,
    require_admin: bool,
) -> CommandResult<AuthResponse> {
    let employee_id = fingerprint::identify_employee(&state.db, &state.paths)
        .await
        .map_err(to_string)?;
    let employee = employee_by_id(&state.db, &employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Fingerprint matched an unknown employee.".to_string())?;

    if !employee.active {
        return Err(format!("{} is inactive.", employee.name));
    }
    if require_admin && !employee.is_admin {
        return Err("This fingerprint does not have admin privilege.".to_string());
    }

    Ok(AuthResponse {
        employee,
        source: "fingerprint".to_string(),
    })
}

#[tauri::command]
pub async fn enroll_fingerprint(
    state: State<'_, AppState>,
    employee_id: String,
    finger: String,
) -> CommandResult<Employee> {
    let employee = employee_by_id(&state.db, &employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Choose a saved employee before enrolling a fingerprint.".to_string())?;
    fingerprint::enroll_employee(&state.db, &state.paths, &employee.id, &finger)
        .await
        .map_err(to_string)?;

    employee_by_id(&state.db, &employee.id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee was enrolled but could not be reloaded.".to_string())
}

#[tauri::command]
pub async fn list_stock_items(state: State<'_, AppState>) -> CommandResult<Vec<StockItem>> {
    let rows = sqlx::query(
        r#"
        SELECT id, item_type, model, stock, location, dimensions, photo_path, notes
        FROM stock_items
        ORDER BY item_type, model COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows
        .into_iter()
        .map(|row| StockItem {
            id: row.get("id"),
            item_type: row.get("item_type"),
            model: row.get("model"),
            stock: row.get("stock"),
            location: row.get("location"),
            dimensions: row.get("dimensions"),
            photo_path: row.get("photo_path"),
            notes: row.get("notes"),
        })
        .collect())
}

#[tauri::command]
pub async fn save_stock_item(state: State<'_, AppState>, input: StockItemInput) -> CommandResult<StockItem> {
    if input.model.trim().is_empty() {
        return Err("Model is required.".to_string());
    }
    let now = crate::db::now_string();
    let id = if let Some(id) = input.id {
        sqlx::query(
            r#"
            UPDATE stock_items
            SET item_type = ?, model = ?, stock = ?, location = ?, dimensions = ?,
                photo_path = ?, notes = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.item_type.trim())
        .bind(input.model.trim())
        .bind(input.stock)
        .bind(input.location.trim())
        .bind(input.dimensions.trim())
        .bind(input.photo_path.trim())
        .bind(input.notes.trim())
        .bind(&now)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO stock_items
                (item_type, model, stock, location, dimensions, photo_path, notes, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(input.item_type.trim())
        .bind(input.model.trim())
        .bind(input.stock)
        .bind(input.location.trim())
        .bind(input.dimensions.trim())
        .bind(input.photo_path.trim())
        .bind(input.notes.trim())
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    stock_item_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn list_cornice_rates(state: State<'_, AppState>) -> CommandResult<Vec<CorniceRate>> {
    let rows = sqlx::query(
        r#"
        SELECT id, series, model, unit_text, unit_value, is_confidential
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
            SET series = ?, model = ?, unit_text = ?, unit_value = ?,
                is_confidential = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.series.trim())
        .bind(input.model.trim())
        .bind(input.unit_text.trim())
        .bind(input.unit_value)
        .bind(input.is_confidential as i64)
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
                (series, model, unit_text, unit_value, is_confidential, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(input.series.trim())
        .bind(input.model.trim())
        .bind(input.unit_text.trim())
        .bind(input.unit_value)
        .bind(input.is_confidential as i64)
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    cornice_rate_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn record_clock_event(
    state: State<'_, AppState>,
    request: ClockRequest,
) -> CommandResult<ClockEvent> {
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
    let now = crate::db::now_string();
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

    let mut needs_review = false;
    let mut note = String::new();
    if request.action == "clock_out" && !has_clock_in {
        needs_review = true;
        note = "Clock-in missing; admin review required.".to_string();
    } else if request.action == "clock_in" && last_action.as_deref() == Some("clock_in") {
        needs_review = true;
        note = "Employee clocked in twice without a clock-out.".to_string();
    } else if request.action == "clock_out" && last_action.as_deref() == Some("clock_out") {
        needs_review = true;
        note = "Employee clocked out twice.".to_string();
    }

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
pub async fn list_admin_alerts(state: State<'_, AppState>) -> CommandResult<Vec<AdminAlert>> {
    refresh_attendance_issues(&state.db).await?;
    let rows = sqlx::query(
        r#"
        SELECT id, severity, kind, message, entity_table, entity_id, resolved, created_at
        FROM admin_notifications
        WHERE resolved = 0
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(alert_from_row).collect())
}

#[tauri::command]
pub async fn resolve_alert(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("UPDATE admin_notifications SET resolved = 1 WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
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

    let (series, unit_text, unit_value, is_custom) = match rate {
        Some(rate) => (rate.series, rate.unit_text, rate.unit_value, false),
        None => (input.series.trim().to_string(), String::new(), None, true),
    };
    let total_units = unit_value.unwrap_or(0.0) * input.lengths as f64;
    let needs_review = is_custom || unit_value.is_none();

    let result = sqlx::query(
        r#"
        INSERT INTO cornice_logs
            (employee_id, log_date, week_start, series, model, lengths, unit_text,
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
    .bind(unit_text)
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
            &format!("Unknown or custom cornice model {} was logged.", input.model.trim()),
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
pub async fn add_overstock(state: State<'_, AppState>, input: OverstockInput) -> CommandResult<OverstockItem> {
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
pub async fn add_delivery(state: State<'_, AppState>, input: DeliveryInput) -> CommandResult<Delivery> {
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

async fn stock_item_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<StockItem> {
    let row = sqlx::query(
        "SELECT id, item_type, model, stock, location, dimensions, photo_path, notes FROM stock_items WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;

    Ok(StockItem {
        id: row.get("id"),
        item_type: row.get("item_type"),
        model: row.get("model"),
        stock: row.get("stock"),
        location: row.get("location"),
        dimensions: row.get("dimensions"),
        photo_path: row.get("photo_path"),
        notes: row.get("notes"),
    })
}

async fn cornice_rate_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<CorniceRate> {
    let row = sqlx::query(
        "SELECT id, series, model, unit_text, unit_value, is_confidential FROM cornice_rates WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;

    Ok(cornice_rate_from_row(row))
}

async fn find_rate_for_model(db: &sqlx::SqlitePool, model: &str) -> Result<Option<CorniceRate>, sqlx::Error> {
    sqlx::query(
        r#"
        SELECT id, series, model, unit_text, unit_value, is_confidential
        FROM cornice_rates
        WHERE lower(model) = lower(?)
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
        unit_text: row.get("unit_text"),
        unit_value: row.get("unit_value"),
        is_confidential: row.get::<i64, _>("is_confidential") != 0,
    }
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

async fn attendance_for_date(db: &sqlx::SqlitePool, date: NaiveDate) -> CommandResult<Vec<AttendanceSummary>> {
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
        let (seconds, needs_review, note) = seconds_from_event_rows(&rows, date == Local::now().date_naive());
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

fn seconds_from_event_rows(rows: &[sqlx::sqlite::SqliteRow], include_open_until_now: bool) -> (i64, bool, String) {
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

async fn refresh_attendance_issues(db: &sqlx::SqlitePool) -> CommandResult<()> {
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
        unit_text: row.get("unit_text"),
        unit_value: row.get("unit_value"),
        total_units: row.get("total_units"),
        weekly_units,
        is_custom: row.get::<i64, _>("is_custom") != 0,
        needs_admin_review: row.get::<i64, _>("needs_admin_review") != 0,
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

fn alert_from_row(row: sqlx::sqlite::SqliteRow) -> AdminAlert {
    AdminAlert {
        id: row.get("id"),
        severity: row.get("severity"),
        kind: row.get("kind"),
        message: row.get("message"),
        entity_table: row.get("entity_table"),
        entity_id: row.get("entity_id"),
        resolved: row.get::<i64, _>("resolved") != 0,
        created_at: row.get("created_at"),
    }
}

fn parse_timestamp(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok()
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
