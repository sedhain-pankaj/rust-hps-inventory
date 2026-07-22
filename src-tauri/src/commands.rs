use base64::{engine::general_purpose, Engine as _};
use chrono::{Local, NaiveDate, NaiveDateTime, Timelike};
use serde_json::{Map, Value};
use std::sync::Arc;

use sqlx::Row;
use tauri::{Emitter, State};

use crate::{
    db::{
        employee_by_id, format_seconds, hash_password, is_legacy_password_hash, list_employees,
        notification, parse_date_or_today, today_string, verify_password, week_start_for, AppState,
        FingerprintEnrollJob,
    },
    fingerprint,
    models::*,
};

type CommandResult<T> = Result<T, String>;

#[derive(Clone, Copy)]
enum AdminColumnKind {
    Text,
    Integer,
    Real,
    Bool,
    Blob,
}

#[derive(Clone, Copy)]
struct AdminColumn {
    name: &'static str,
    label: &'static str,
    kind: AdminColumnKind,
    editable: bool,
    protected: bool,
}

struct AdminTable {
    name: &'static str,
    label: &'static str,
    columns: &'static [AdminColumn],
}

const EMPLOYEE_COLUMNS: &[AdminColumn] = &[
    col("id", "Employee ID", AdminColumnKind::Text),
    col("name", "Name", AdminColumnKind::Text),
    col("finger", "Finger", AdminColumnKind::Text),
    col("active", "Active", AdminColumnKind::Bool),
    col("is_admin", "Admin", AdminColumnKind::Bool),
    protected_col("password_hash", "Password Hash", AdminColumnKind::Text),
    col("created_at", "Created", AdminColumnKind::Text),
    col("updated_at", "Updated", AdminColumnKind::Text),
];
const EMPLOYEE_PERMISSION_COLUMNS: &[AdminColumn] = &[
    col("employee_id", "Employee ID", AdminColumnKind::Text),
    col("permission", "Permission", AdminColumnKind::Text),
];
const FINGERPRINT_COLUMNS: &[AdminColumn] = &[
    col("employee_id", "Employee ID", AdminColumnKind::Text),
    col("finger", "Finger", AdminColumnKind::Text),
    protected_col("template", "Template Blob", AdminColumnKind::Blob),
    col("updated_at", "Updated", AdminColumnKind::Text),
];
const CORNICE_RATE_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("series", "Series", AdminColumnKind::Text),
    col("model", "Model", AdminColumnKind::Text),
    col("unit_text", "Unit Text", AdminColumnKind::Text),
    col("unit_value", "Unit Value", AdminColumnKind::Real),
    col("is_confidential", "Confidential", AdminColumnKind::Bool),
    col("updated_at", "Updated", AdminColumnKind::Text),
];
const STOCK_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("item_type", "Type", AdminColumnKind::Text),
    col("model", "Model", AdminColumnKind::Text),
    col("stock", "Stock", AdminColumnKind::Integer),
    col("location", "Location", AdminColumnKind::Text),
    col("dimensions", "Dimensions", AdminColumnKind::Text),
    col("photo_path", "Photo/Asset", AdminColumnKind::Text),
    col("notes", "Notes", AdminColumnKind::Text),
    col("updated_at", "Updated", AdminColumnKind::Text),
];
const TIME_CLOCK_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("employee_id", "Employee ID", AdminColumnKind::Text),
    col("work_date", "Work Date", AdminColumnKind::Text),
    col("action", "Action", AdminColumnKind::Text),
    col("timestamp", "Timestamp", AdminColumnKind::Text),
    col("source", "Source", AdminColumnKind::Text),
    col("needs_admin_review", "Review", AdminColumnKind::Bool),
    col("note", "Note", AdminColumnKind::Text),
];
const CORNICE_LOG_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("employee_id", "Employee ID", AdminColumnKind::Text),
    col("log_date", "Log Date", AdminColumnKind::Text),
    col("week_start", "Week Start", AdminColumnKind::Text),
    col("series", "Series", AdminColumnKind::Text),
    col("model", "Model", AdminColumnKind::Text),
    col("lengths", "Lengths", AdminColumnKind::Integer),
    col("unit_text", "Unit Text", AdminColumnKind::Text),
    col("unit_value", "Unit Value", AdminColumnKind::Real),
    col("total_units", "Total Units", AdminColumnKind::Real),
    col("is_custom", "Custom", AdminColumnKind::Bool),
    col("needs_admin_review", "Review", AdminColumnKind::Bool),
    col("created_at", "Created", AdminColumnKind::Text),
];
const PRODUCTION_LOG_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("employee_id", "Employee ID", AdminColumnKind::Text),
    col("log_date", "Log Date", AdminColumnKind::Text),
    col("item", "Item", AdminColumnKind::Text),
    col("quantity", "Quantity", AdminColumnKind::Integer),
    col("notes", "Notes", AdminColumnKind::Text),
    col("created_at", "Created", AdminColumnKind::Text),
];
const OVERSTOCK_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("model", "Model", AdminColumnKind::Text),
    col("quantity", "Quantity", AdminColumnKind::Integer),
    col("aisle", "Aisle", AdminColumnKind::Text),
    col("notes", "Notes", AdminColumnKind::Text),
    col("updated_by", "Updated By", AdminColumnKind::Text),
    col("updated_at", "Updated", AdminColumnKind::Text),
];
const DELIVERY_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("driver_id", "Driver ID", AdminColumnKind::Text),
    col("delivery_date", "Delivery Date", AdminColumnKind::Text),
    col("address", "Address", AdminColumnKind::Text),
    col("items", "Items", AdminColumnKind::Text),
    col("notes", "Notes", AdminColumnKind::Text),
    col("created_at", "Created", AdminColumnKind::Text),
];
const NOTIFICATION_COLUMNS: &[AdminColumn] = &[
    readonly_col("id", "ID", AdminColumnKind::Integer),
    col("severity", "Severity", AdminColumnKind::Text),
    col("kind", "Kind", AdminColumnKind::Text),
    col("message", "Message", AdminColumnKind::Text),
    col("entity_table", "Entity Table", AdminColumnKind::Text),
    col("entity_id", "Entity ID", AdminColumnKind::Integer),
    col("resolved", "Resolved", AdminColumnKind::Bool),
    col("created_at", "Created", AdminColumnKind::Text),
];
const APP_META_COLUMNS: &[AdminColumn] = &[
    col("key", "Key", AdminColumnKind::Text),
    col("value", "Value", AdminColumnKind::Text),
];
const APP_ASSET_COLUMNS: &[AdminColumn] = &[
    col("key", "Key", AdminColumnKind::Text),
    col("name", "Name", AdminColumnKind::Text),
    col("media_type", "Media Type", AdminColumnKind::Text),
    protected_col("content", "Content Blob", AdminColumnKind::Blob),
    col("updated_at", "Updated", AdminColumnKind::Text),
];

const ADMIN_TABLES: &[AdminTable] = &[
    AdminTable {
        name: "employees",
        label: "Employees",
        columns: EMPLOYEE_COLUMNS,
    },
    AdminTable {
        name: "employee_permissions",
        label: "Employee Permissions",
        columns: EMPLOYEE_PERMISSION_COLUMNS,
    },
    AdminTable {
        name: "fingerprint_templates",
        label: "Fingerprint Templates",
        columns: FINGERPRINT_COLUMNS,
    },
    AdminTable {
        name: "cornice_rates",
        label: "Cornice Rates",
        columns: CORNICE_RATE_COLUMNS,
    },
    AdminTable {
        name: "stock_items",
        label: "Stock Items",
        columns: STOCK_COLUMNS,
    },
    AdminTable {
        name: "time_clock_events",
        label: "Time Clock Events",
        columns: TIME_CLOCK_COLUMNS,
    },
    AdminTable {
        name: "cornice_logs",
        label: "Cornice Logs",
        columns: CORNICE_LOG_COLUMNS,
    },
    AdminTable {
        name: "production_logs",
        label: "Production Logs",
        columns: PRODUCTION_LOG_COLUMNS,
    },
    AdminTable {
        name: "overstock_locations",
        label: "Overstock Locations",
        columns: OVERSTOCK_COLUMNS,
    },
    AdminTable {
        name: "deliveries",
        label: "Deliveries",
        columns: DELIVERY_COLUMNS,
    },
    AdminTable {
        name: "admin_notifications",
        label: "Admin Notifications",
        columns: NOTIFICATION_COLUMNS,
    },
    AdminTable {
        name: "app_meta",
        label: "App Metadata",
        columns: APP_META_COLUMNS,
    },
    AdminTable {
        name: "app_assets",
        label: "App Assets",
        columns: APP_ASSET_COLUMNS,
    },
];

const fn col(name: &'static str, label: &'static str, kind: AdminColumnKind) -> AdminColumn {
    AdminColumn {
        name,
        label,
        kind,
        editable: true,
        protected: false,
    }
}

const fn readonly_col(
    name: &'static str,
    label: &'static str,
    kind: AdminColumnKind,
) -> AdminColumn {
    AdminColumn {
        name,
        label,
        kind,
        editable: false,
        protected: false,
    }
}

const fn protected_col(
    name: &'static str,
    label: &'static str,
    kind: AdminColumnKind,
) -> AdminColumn {
    AdminColumn {
        name,
        label,
        kind,
        editable: false,
        protected: true,
    }
}

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
pub async fn get_asset_data_url(state: State<'_, AppState>, key: String) -> CommandResult<String> {
    let row = sqlx::query("SELECT media_type, content FROM app_assets WHERE key = ?")
        .bind(key.trim())
        .fetch_optional(&state.db)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Asset was not found in the database.".to_string())?;
    let media_type: String = row.get("media_type");
    let content: Vec<u8> = row.get("content");
    Ok(format!(
        "data:{media_type};base64,{}",
        general_purpose::STANDARD.encode(content)
    ))
}

#[tauri::command]
pub async fn list_staff(
    state: State<'_, AppState>,
    include_inactive: bool,
) -> CommandResult<Vec<Employee>> {
    list_employees(&state.db, include_inactive)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub async fn save_employee(
    state: State<'_, AppState>,
    input: EmployeeInput,
) -> CommandResult<Employee> {
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

    // Update staff_category if provided and valid
    let category = input.staff_category.trim();
    if !category.is_empty()
        && ["cornice_hand", "storekeeper", "non_cornice", "driver", "helper"]
            .iter()
            .any(|c| *c == category)
    {
        sqlx::query("UPDATE employees SET staff_category = ? WHERE id = ?")
            .bind(category)
            .bind(input.id.trim())
            .execute(&state.db)
            .await
            .map_err(to_string)?;
    }

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
    let submitted_password = password.trim().to_string();
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
        if stored
            .as_deref()
            .map(|hash| verify_password(hash, &submitted_password))
            .unwrap_or(false)
        {
            let id: String = row.get("id");
            if stored
                .as_deref()
                .map(is_legacy_password_hash)
                .unwrap_or(false)
            {
                let upgraded = hash_password(&submitted_password);
                sqlx::query("UPDATE employees SET password_hash = ?, updated_at = ? WHERE id = ?")
                    .bind(upgraded)
                    .bind(crate::db::now_string())
                    .bind(&id)
                    .execute(&state.db)
                    .await
                    .map_err(to_string)?;
            }
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
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    employee_id: String,
    finger: String,
) -> CommandResult<FingerprintEnrollResponse> {
    let employee = employee_by_id(&state.db, &employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Choose a saved employee before enrolling a fingerprint.".to_string())?;
    let progress_buffer = state.fingerprint_progress.clone();
    if let Ok(mut lines) = progress_buffer.lock() {
        lines.clear();
    }
    let app_for_progress = app.clone();
    let progress = Arc::new(move |line: String| {
        if let Ok(mut lines) = progress_buffer.lock() {
            lines.push(line.clone());
        }
        let app_for_emit = app_for_progress.clone();
        tauri::async_runtime::spawn(async move {
            let _ = app_for_emit.emit("fingerprint_progress", line.clone());
            let _ = app_for_emit.emit("fingerprint-progress", line);
        });
    });
    let messages = fingerprint::enroll_employee(
        &state.db,
        &state.paths,
        &employee.id,
        &finger,
        Some(progress),
    )
    .await
    .map_err(to_string)?;

    let employee = employee_by_id(&state.db, &employee.id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee was enrolled but could not be reloaded.".to_string())?;
    Ok(FingerprintEnrollResponse { employee, messages })
}

#[tauri::command]
pub async fn start_fingerprint_enroll(
    state: State<'_, AppState>,
    employee_id: String,
    finger: String,
) -> CommandResult<FingerprintEnrollStartResponse> {
    let employee = employee_by_id(&state.db, &employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Choose a saved employee before enrolling a fingerprint.".to_string())?;
    let finger = if finger.trim().is_empty() {
        "right-index".to_string()
    } else {
        finger.trim().to_string()
    };
    let job_id = state.next_enroll_job_id();
    {
        let mut jobs = state
            .enroll_jobs
            .lock()
            .map_err(|_| "Could not create enrollment job.".to_string())?;
        jobs.insert(
            job_id.clone(),
            FingerprintEnrollJob {
                employee_id: employee.id.clone(),
                lines: vec!["Starting enrollment. Follow the reader prompts.".to_string()],
                done: false,
                error: None,
            },
        );
    }

    let db = state.db.clone();
    let paths = state.paths.clone();
    let jobs = state.enroll_jobs.clone();
    let job_id_for_task = job_id.clone();
    let employee_id_for_task = employee.id.clone();
    tauri::async_runtime::spawn(async move {
        let progress_jobs = jobs.clone();
        let progress_job_id = job_id_for_task.clone();
        let progress = Arc::new(move |line: String| {
            if let Ok(mut all_jobs) = progress_jobs.lock() {
                if let Some(job) = all_jobs.get_mut(&progress_job_id) {
                    job.lines.push(line);
                }
            }
        });

        let result = fingerprint::enroll_employee(
            &db,
            &paths,
            &employee_id_for_task,
            &finger,
            Some(progress),
        )
        .await;

        if let Ok(mut all_jobs) = jobs.lock() {
            if let Some(job) = all_jobs.get_mut(&job_id_for_task) {
                match result {
                    Ok(messages) => {
                        if !messages.is_empty() {
                            job.lines = messages;
                        }
                        job.done = true;
                    }
                    Err(error) => {
                        job.error = Some(error.to_string());
                        job.done = true;
                    }
                }
            }
        }
    });

    Ok(FingerprintEnrollStartResponse { job_id })
}

#[tauri::command]
pub async fn poll_fingerprint_enroll(
    state: State<'_, AppState>,
    job_id: String,
    from_index: Option<usize>,
) -> CommandResult<FingerprintEnrollStatusResponse> {
    let start = from_index.unwrap_or(0);
    let (employee_id, done, error, next_index, lines) = {
        let jobs = state
            .enroll_jobs
            .lock()
            .map_err(|_| "Could not read enrollment job.".to_string())?;
        let job = jobs
            .get(&job_id)
            .ok_or_else(|| "Enrollment job was not found.".to_string())?;
        let next_index = job.lines.len();
        let lines = if start < next_index {
            job.lines[start..].to_vec()
        } else {
            Vec::new()
        };
        (
            job.employee_id.clone(),
            job.done,
            job.error.clone(),
            next_index,
            lines,
        )
    };

    let employee = if done && error.is_none() {
        employee_by_id(&state.db, &employee_id)
            .await
            .map_err(to_string)?
    } else {
        None
    };
    let state_name = if !done {
        "running"
    } else if error.is_some() {
        "failed"
    } else {
        "done"
    };
    Ok(FingerprintEnrollStatusResponse {
        job_id,
        state: state_name.to_string(),
        lines,
        next_index,
        error,
        employee,
    })
}

#[tauri::command]
pub fn read_fingerprint_progress(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    let lines = state
        .fingerprint_progress
        .lock()
        .map_err(|_| "Could not read fingerprint progress.".to_string())?
        .clone();
    Ok(lines)
}

#[tauri::command]
pub fn clear_fingerprint_progress(state: State<'_, AppState>) -> CommandResult<()> {
    state
        .fingerprint_progress
        .lock()
        .map_err(|_| "Could not clear fingerprint progress.".to_string())?
        .clear();
    Ok(())
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
pub async fn save_stock_item(
    state: State<'_, AppState>,
    input: StockItemInput,
) -> CommandResult<StockItem> {
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
pub async fn list_admin_tables() -> CommandResult<Vec<AdminTableInfo>> {
    Ok(ADMIN_TABLES
        .iter()
        .map(|table| AdminTableInfo {
            name: table.name.to_string(),
            label: table.label.to_string(),
        })
        .collect())
}

#[tauri::command]
pub async fn list_admin_table_rows(
    state: State<'_, AppState>,
    table: String,
) -> CommandResult<AdminTableData> {
    let config = admin_table_config(&table)?;
    let select_columns = config
        .columns
        .iter()
        .map(|column| column.name)
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT rowid AS __rowid, {select_columns} FROM {} ORDER BY rowid DESC LIMIT 500",
        config.name
    );
    let rows = sqlx::query(&sql)
        .fetch_all(&state.db)
        .await
        .map_err(to_string)?;

    let mut output_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let mut values = Map::new();
        for column in config.columns {
            values.insert(column.name.to_string(), admin_cell_value(&row, column));
        }
        output_rows.push(AdminTableRow {
            rowid: row.get("__rowid"),
            values: Value::Object(values),
        });
    }

    Ok(AdminTableData {
        table: config.name.to_string(),
        label: config.label.to_string(),
        columns: admin_column_info(config),
        rows: output_rows,
    })
}

#[tauri::command]
pub async fn save_admin_table_row(
    state: State<'_, AppState>,
    input: AdminTableSaveInput,
) -> CommandResult<AdminTableData> {
    let config = admin_table_config(&input.table)?;
    let values = input
        .values
        .as_object()
        .ok_or_else(|| "Row values must be an object.".to_string())?;
    let editable_columns = config
        .columns
        .iter()
        .filter(|column| column.editable && !column.protected && values.contains_key(column.name))
        .copied()
        .collect::<Vec<_>>();

    if editable_columns.is_empty() {
        return Err("No editable values were provided.".to_string());
    }

    if let Some(rowid) = input.rowid {
        let assignments = editable_columns
            .iter()
            .map(|column| format!("{} = ?", column.name))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE {} SET {assignments} WHERE rowid = ?", config.name);
        let mut query = sqlx::query(&sql);
        for column in &editable_columns {
            query = bind_admin_value(query, column.kind, values.get(column.name));
        }
        query
            .bind(rowid)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
    } else {
        let names = editable_columns
            .iter()
            .map(|column| column.name)
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = std::iter::repeat("?")
            .take(editable_columns.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO {} ({names}) VALUES ({placeholders})",
            config.name
        );
        let mut query = sqlx::query(&sql);
        for column in &editable_columns {
            query = bind_admin_value(query, column.kind, values.get(column.name));
        }
        query.execute(&state.db).await.map_err(to_string)?;
    }

    list_admin_table_rows(state, config.name.to_string()).await
}

#[tauri::command]
pub async fn delete_admin_table_row(
    state: State<'_, AppState>,
    table: String,
    rowid: i64,
) -> CommandResult<AdminTableData> {
    let config = admin_table_config(&table)?;
    let sql = format!("DELETE FROM {} WHERE rowid = ?", config.name);
    sqlx::query(&sql)
        .bind(rowid)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    list_admin_table_rows(state, config.name.to_string()).await
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

// ==================== Mould Inventory ====================

#[tauri::command]
pub async fn list_mould_inventory(state: State<'_, AppState>) -> CommandResult<Vec<MouldInventory>> {
    let rows = sqlx::query(
        r#"
        SELECT id, mould_name, storage_location, notes, updated_at
        FROM mould_inventory
        ORDER BY mould_name COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(mould_from_row).collect())
}

#[tauri::command]
pub async fn save_mould_inventory(
    state: State<'_, AppState>,
    input: MouldInventoryInput,
) -> CommandResult<MouldInventory> {
    if input.mould_name.trim().is_empty() {
        return Err("Mould name is required.".to_string());
    }
    let now = crate::db::now_string();
    let id = if let Some(id) = input.id {
        sqlx::query(
            r#"
            UPDATE mould_inventory
            SET mould_name = ?, storage_location = ?, notes = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.mould_name.trim())
        .bind(input.storage_location.trim())
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
            INSERT INTO mould_inventory (mould_name, storage_location, notes, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(input.mould_name.trim())
        .bind(input.storage_location.trim())
        .bind(input.notes.trim())
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    mould_by_id(&state.db, id).await
}

// ==================== Cornice Stock ====================

#[tauri::command]
pub async fn list_cornice_stock(state: State<'_, AppState>) -> CommandResult<Vec<CorniceStock>> {
    let rows = sqlx::query(
        r#"
        SELECT id, model, aisle, quantity_in_stock, quantity_reserved, remarks, updated_at
        FROM cornice_stock
        ORDER BY model COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(cornice_stock_from_row).collect())
}

#[tauri::command]
pub async fn save_cornice_stock(
    state: State<'_, AppState>,
    input: CorniceStockInput,
) -> CommandResult<CorniceStock> {
    if input.model.trim().is_empty() {
        return Err("Model is required.".to_string());
    }
    let now = crate::db::now_string();
    let id = if let Some(id) = input.id {
        sqlx::query(
            r#"
            UPDATE cornice_stock
            SET model = ?, aisle = ?, quantity_in_stock = ?, quantity_reserved = ?, remarks = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.model.trim())
        .bind(input.aisle.trim())
        .bind(input.quantity_in_stock)
        .bind(input.quantity_reserved)
        .bind(input.remarks.trim())
        .bind(&now)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO cornice_stock (model, aisle, quantity_in_stock, quantity_reserved, remarks, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(input.model.trim())
        .bind(input.aisle.trim())
        .bind(input.quantity_in_stock)
        .bind(input.quantity_reserved)
        .bind(input.remarks.trim())
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    cornice_stock_by_id(&state.db, id).await
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

// ==================== Fuzzy Search Cornice Rates ====================

#[tauri::command]
pub async fn search_cornice_rates(
    state: State<'_, AppState>,
    request: SearchCorniceRatesRequest,
) -> CommandResult<SearchCorniceRatesResponse> {
    let query = request.query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Ok(SearchCorniceRatesResponse { matches: Vec::new() });
    }

    // Get all rates and do fuzzy matching in Rust (Levenshtein-like scoring)
    let rows = sqlx::query(
        r#"
        SELECT id, series, model, unit_text, unit_value
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

        // Scoring: exact match, starts_with, contains, levenshtein-like
        let score = if model_lower == query {
            1000
        } else if model_lower.starts_with(&query) {
            500
        } else if model_lower.contains(&query) {
            200
        } else {
            // Character-level similarity score
            let common: usize = query.chars().filter(|c| model_lower.contains(*c)).count();
            if common >= query.len().saturating_sub(1) && common > 0 {
                common as u32 * 10
            } else {
                0
            }
        };

        if score > 0 {
            matches.push(CorniceRateMatch {
                id: row.get("id"),
                series: row.get("series"),
                model,
                unit_text: row.get("unit_text"),
                unit_value: row.get("unit_value"),
                score,
            });
        }
    }

    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.model.cmp(&b.model)));
    matches.truncate(20);

    Ok(SearchCorniceRatesResponse { matches })
}

// ==================== Payroll Engine ====================

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

    let mut total_units_known: f64 = 0.0;
    let mut unknown_details: Vec<UnknownRateDetail> = Vec::new();
    for row in &cornice_rows {
        let is_custom: i64 = row.get("is_custom");
        let unit_value: Option<f64> = row.get("unit_value");
        let lengths: i64 = row.get("lengths");

        if is_custom != 0 || unit_value.is_none() {
            let model: String = row.get("model");
            // Merge with existing unknown detail for same model
            if let Some(existing) = unknown_details.iter_mut().find(|d| d.model == model) {
                existing.quantity += lengths;
            } else {
                unknown_details.push(UnknownRateDetail { model, quantity: lengths });
            }
        } else if let Some(uv) = unit_value {
            total_units_known += uv * lengths as f64;
        }
    }

    let total_units_unknown: f64 = unknown_details.iter().map(|d| d.quantity as f64).sum();

    // Attendance-adjusted threshold (§5.4)
    let (unit_threshold, threshold_note, needs_review_hours) = if (39.0..=41.0).contains(&total_hours) || total_hours == 0.0 {
        (180.0, "Standard week (39-41 hr band or no hours)".to_string(), false)
    } else {
        let prorated = total_hours * 4.5;
        (prorated, format!("Prorated: {:.1} hrs × 4.5 = {:.1} units (outside 39-41 hr band)", total_hours, prorated), true)
    };

    // Base pay
    let base_pay = 1140.0_f64;

    // Extra unit calculation
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
            unit_threshold
        );
        (None, 0.0, eq, "unresolved".to_string())
    } else {
        let extra_units = (total_units_known - unit_threshold).max(0.0);
        let eup = extra_units * 3.80;
        let gp = base_pay + eup;
        let eq = format!(
            "${:.2} (base) + ${:.2} ({:.0} extra units × $3.80) = ${:.2}",
            base_pay, eup, extra_units, gp
        );
        (Some(gp), eup, eq, if needs_review_hours { "review" } else { "final" }.to_string())
    };

    // Persist payroll period
    let week_end_str = week_end.format("%Y-%m-%d").to_string();
    let gross_for_db = gross_pay.unwrap_or(0.0);
    let needs_review = needs_review_hours || status == "unresolved";

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
    .bind(total_units_known)
    .bind(unit_threshold)
    .bind(base_pay)
    .bind(extra_unit_pay)
    .bind(gross_for_db)
    .bind(&status)
    .bind(&pay_equation)
    .bind(needs_review as i64)
    .bind(crate::db::now_string())
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    // Raise alert for unresolved or outside-band
    if status == "unresolved" {
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
            &format!("{} worked {:.1} hrs for week {} (outside 39-41 band). Prorated threshold: {:.0} units. Admin review needed.", employee.name, total_hours, req_week_start.format("%Y-%m-%d"), unit_threshold),
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
        total_units_known,
        total_units_unknown,
        unknown_rate_details: unknown_details,
        unit_threshold,
        threshold_note,
        base_pay,
        extra_unit_pay,
        gross_pay,
        pay_equation,
        status,
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
            SET unit_value = ?, is_confidential = 0, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.unit_value)
        .bind(&now)
        .bind(existing_id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        existing_id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO cornice_rates (series, model, unit_text, unit_value, is_confidential, updated_at)
            VALUES (?, ?, ?, ?, 0, ?)
            "#,
        )
        .bind(&series)
        .bind(model)
        .bind(format!("{:.2} units/m", input.unit_value))
        .bind(input.unit_value)
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
        SET unit_value = ?, total_units = lengths * ?, is_custom = 0, needs_admin_review = 0, updated_at = ?
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

    let mut total_units_known: f64 = 0.0;
    let mut unknown_details: Vec<UnknownRateDetail> = Vec::new();
    for row in &cornice_rows {
        let is_custom: i64 = row.get("is_custom");
        let unit_value: Option<f64> = row.get("unit_value");
        let lengths: i64 = row.get("lengths");

        if is_custom != 0 || unit_value.is_none() {
            let model: String = row.get("model");
            if let Some(existing) = unknown_details.iter_mut().find(|d| d.model == model) {
                existing.quantity += lengths;
            } else {
                unknown_details.push(UnknownRateDetail { model, quantity: lengths });
            }
        } else if let Some(uv) = unit_value {
            total_units_known += uv * lengths as f64;
        }
    }

    let (unit_threshold, threshold_note, needs_review_hours) = if (39.0..=41.0).contains(&total_hours) || total_hours == 0.0 {
        (180.0, "Standard week".to_string(), false)
    } else {
        let prorated = total_hours * 4.5;
        (prorated, format!("Prorated: {:.1} hrs × 4.5", total_hours), true)
    };

    let base_pay = 1140.0_f64;
    let (gross_pay, extra_unit_pay, pay_equation, status) = if !unknown_details.is_empty() {
        let known_part = total_units_known.floor() as i64;
        let unknown_parts: Vec<String> = unknown_details.iter().map(|d| format!("{}×{}", d.quantity, d.model)).collect();
        let eq = format!("{} units + {} − {:.0} (base)", known_part, unknown_parts.join(" + "), unit_threshold);
        (None, 0.0, eq, "unresolved".to_string())
    } else {
        let extra_units = (total_units_known - unit_threshold).max(0.0);
        let eup = extra_units * 3.80;
        let gp = base_pay + eup;
        let eq = format!("${:.2} + ${:.2} ({:.0} extra × $3.80) = ${:.2}", base_pay, eup, extra_units, gp);
        (Some(gp), eup, eq, if needs_review_hours { "review" } else { "final" }.to_string())
    };

    Ok(PayrollWeekResponse {
        employee_id: employee_id.to_string(),
        employee_name: employee_name.to_string(),
        week_start: week_start.format("%Y-%m-%d").to_string(),
        week_end: week_end.format("%Y-%m-%d").to_string(),
        total_hours,
        total_units_known,
        total_units_unknown: unknown_details.iter().map(|d| d.quantity as f64).sum(),
        unknown_rate_details: unknown_details,
        unit_threshold,
        threshold_note,
        base_pay,
        extra_unit_pay,
        gross_pay,
        pay_equation,
        status: status.clone(),
        needs_admin_review: needs_review_hours || status == "unresolved",
    })
}

// ==================== Dispatch Orders ====================

#[tauri::command]
pub async fn list_dispatch_orders(
    state: State<'_, AppState>,
    status: Option<String>,
) -> CommandResult<Vec<DispatchOrder>> {
    if let Some(s) = status {
        let rows = sqlx::query(
            r#"
            SELECT d.*, e.name AS created_by_name, de.name AS delivered_by_name
            FROM dispatch_orders d
            JOIN employees e ON e.id = d.created_by
            LEFT JOIN employees de ON de.id = d.delivered_by
            WHERE d.status = ?
            ORDER BY d.created_at DESC, d.id DESC
            "#,
        )
        .bind(s)
        .fetch_all(&state.db)
        .await
        .map_err(to_string)?;
        return Ok(rows.into_iter().map(dispatch_order_from_row).collect());
    }

    let rows = sqlx::query(
        r#"
        SELECT d.*, e.name AS created_by_name, de.name AS delivered_by_name
        FROM dispatch_orders d
        JOIN employees e ON e.id = d.created_by
        LEFT JOIN employees de ON de.id = d.delivered_by
        ORDER BY d.created_at DESC, d.id DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;
    Ok(rows.into_iter().map(dispatch_order_from_row).collect())
}

#[tauri::command]
pub async fn create_dispatch_order(
    state: State<'_, AppState>,
    input: DispatchOrderInput,
    created_by: String,
) -> CommandResult<DispatchOrder> {
    if input.cornice_model.trim().is_empty() {
        return Err("Cornice model is required.".to_string());
    }
    if input.quantity <= 0 {
        return Err("Quantity must be positive.".to_string());
    }
    if input.delivery_location.trim().is_empty() {
        return Err("Delivery location is required.".to_string());
    }

    let now = crate::db::now_string();
    let result = sqlx::query(
        r#"
        INSERT INTO dispatch_orders (cornice_model, quantity, delivery_location, status, created_by, remarks, created_at)
        VALUES (?, ?, ?, 'pending', ?, ?, ?)
        "#,
    )
    .bind(input.cornice_model.trim())
    .bind(input.quantity)
    .bind(input.delivery_location.trim())
    .bind(created_by.trim())
    .bind(input.remarks.trim())
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    dispatch_order_by_id(&state.db, result.last_insert_rowid()).await
}

#[tauri::command]
pub async fn update_dispatch_order(
    state: State<'_, AppState>,
    input: DispatchOrderInput,
    updated_by: String,
) -> CommandResult<DispatchOrder> {
    let id = input.id.ok_or_else(|| "Order ID is required.".to_string())?;

    let mut assignments = Vec::new();
    let mut binds: Vec<&str> = Vec::new();

    if let Some(status) = &input.status {
        if !["pending", "in_progress", "delivered"].contains(&status.trim()) {
            return Err("Status must be 'pending', 'in_progress', or 'delivered'.".to_string());
        }
        assignments.push("status = ?");
        binds.push(status.trim());
    }
    if !input.remarks.trim().is_empty() {
        assignments.push("remarks = ?");
        binds.push(input.remarks.trim());
    }

    // If marking as delivered, set delivered_by and delivered_at
    let now_str = crate::db::now_string();
    if input.status.as_deref() == Some("delivered") {
        assignments.push("delivered_by = ?");
        binds.push(updated_by.trim());
        assignments.push("delivered_at = ?");
        binds.push(&now_str);
    }

    if assignments.is_empty() {
        return dispatch_order_by_id(&state.db, id).await;
    }

    let placeholders = assignments.join(", ");
    let sql = format!("UPDATE dispatch_orders SET {placeholders} WHERE id = ?");
    let mut query = sqlx::query(&sql);
    for b in binds {
        query = query.bind(b);
    }
    query.bind(id).execute(&state.db).await.map_err(to_string)?;

    dispatch_order_by_id(&state.db, id).await
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

fn dispatch_order_from_row(row: sqlx::sqlite::SqliteRow) -> DispatchOrder {
    DispatchOrder {
        id: row.get("id"),
        cornice_model: row.get("cornice_model"),
        quantity: row.get("quantity"),
        delivery_location: row.get("delivery_location"),
        status: row.get("status"),
        created_by_id: row.get("created_by"),
        created_by_name: row.get("created_by_name"),
        delivered_by_name: row.get("delivered_by_name"),
        delivered_at: row.get("delivered_at"),
        remarks: row.get("remarks"),
        created_at: row.get("created_at"),
    }
}

async fn dispatch_order_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<DispatchOrder> {
    let row = sqlx::query(
        r#"
        SELECT d.*, e.name AS created_by_name, de.name AS delivered_by_name
        FROM dispatch_orders d
        JOIN employees e ON e.id = d.created_by
        LEFT JOIN employees de ON de.id = d.delivered_by
        WHERE d.id = ?
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(dispatch_order_from_row(row))
}

fn mould_from_row(row: sqlx::sqlite::SqliteRow) -> MouldInventory {
    MouldInventory {
        id: row.get("id"),
        mould_name: row.get("mould_name"),
        storage_location: row.get("storage_location"),
        notes: row.get("notes"),
        updated_at: row.get("updated_at"),
    }
}

async fn mould_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<MouldInventory> {
    let row = sqlx::query(
        "SELECT id, mould_name, storage_location, notes, updated_at FROM mould_inventory WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(mould_from_row(row))
}

fn cornice_stock_from_row(row: sqlx::sqlite::SqliteRow) -> CorniceStock {
    CorniceStock {
        id: row.get("id"),
        model: row.get("model"),
        aisle: row.get("aisle"),
        quantity_in_stock: row.get("quantity_in_stock"),
        quantity_reserved: row.get("quantity_reserved"),
        remarks: row.get("remarks"),
        updated_at: row.get("updated_at"),
    }
}

async fn cornice_stock_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<CorniceStock> {
    let row = sqlx::query(
        "SELECT id, model, aisle, quantity_in_stock, quantity_reserved, remarks, updated_at FROM cornice_stock WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(cornice_stock_from_row(row))
}

fn admin_table_config(name: &str) -> CommandResult<&'static AdminTable> {
    ADMIN_TABLES
        .iter()
        .find(|table| table.name == name.trim())
        .ok_or_else(|| "Unknown or unsupported database table.".to_string())
}

fn admin_column_info(config: &AdminTable) -> Vec<AdminColumnInfo> {
    config
        .columns
        .iter()
        .map(|column| AdminColumnInfo {
            name: column.name.to_string(),
            label: column.label.to_string(),
            kind: match column.kind {
                AdminColumnKind::Text => "text",
                AdminColumnKind::Integer => "integer",
                AdminColumnKind::Real => "real",
                AdminColumnKind::Bool => "bool",
                AdminColumnKind::Blob => "blob",
            }
            .to_string(),
            editable: column.editable,
            protected: column.protected,
        })
        .collect()
}

fn admin_cell_value(row: &sqlx::sqlite::SqliteRow, column: &AdminColumn) -> Value {
    if column.protected {
        return match column.kind {
            AdminColumnKind::Blob => {
                let bytes: Option<Vec<u8>> = row.try_get(column.name).ok();
                bytes
                    .map(|bytes| Value::String(format!("BLOB {} bytes", bytes.len())))
                    .unwrap_or(Value::Null)
            }
            _ => {
                let present = row
                    .try_get::<Option<String>, _>(column.name)
                    .ok()
                    .flatten()
                    .map(|value| !value.is_empty())
                    .unwrap_or(false);
                if present {
                    Value::String("[protected]".to_string())
                } else {
                    Value::Null
                }
            }
        };
    }

    match column.kind {
        AdminColumnKind::Text => row
            .try_get::<Option<String>, _>(column.name)
            .ok()
            .flatten()
            .map(Value::String)
            .unwrap_or(Value::Null),
        AdminColumnKind::Integer => row
            .try_get::<Option<i64>, _>(column.name)
            .ok()
            .flatten()
            .map(|value| Value::Number(value.into()))
            .unwrap_or(Value::Null),
        AdminColumnKind::Real => row
            .try_get::<Option<f64>, _>(column.name)
            .ok()
            .flatten()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        AdminColumnKind::Bool => row
            .try_get::<Option<i64>, _>(column.name)
            .ok()
            .flatten()
            .map(|value| Value::Bool(value != 0))
            .unwrap_or(Value::Null),
        AdminColumnKind::Blob => row
            .try_get::<Option<Vec<u8>>, _>(column.name)
            .ok()
            .flatten()
            .map(|bytes| Value::String(format!("BLOB {} bytes", bytes.len())))
            .unwrap_or(Value::Null),
    }
}

fn bind_admin_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    kind: AdminColumnKind,
    value: Option<&Value>,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match kind {
        AdminColumnKind::Text => query.bind(value.and_then(json_to_string)),
        AdminColumnKind::Integer => query.bind(value.and_then(json_to_i64)),
        AdminColumnKind::Real => query.bind(value.and_then(json_to_f64)),
        AdminColumnKind::Bool => query.bind(value.and_then(json_to_bool).map(|value| value as i64)),
        AdminColumnKind::Blob => query,
    }
}

fn json_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) if text.is_empty() => Some(String::new()),
        Value::String(text) => Some(text.clone()),
        Value::Bool(value) => Some(if *value { "1" } else { "0" }.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => Some(value.to_string()),
    }
}

fn json_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Null => None,
        Value::Number(number) => number.as_i64(),
        Value::String(text) if text.trim().is_empty() => None,
        Value::String(text) => text.trim().parse().ok(),
        Value::Bool(value) => Some(*value as i64),
        _ => None,
    }
}

fn json_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Null => None,
        Value::Number(number) => number.as_f64(),
        Value::String(text) if text.trim().is_empty() => None,
        Value::String(text) => text.trim().parse().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn json_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Null => None,
        Value::Bool(value) => Some(*value),
        Value::Number(number) => number.as_i64().map(|value| value != 0),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "" => None,
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
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

async fn find_rate_for_model(
    db: &sqlx::SqlitePool,
    model: &str,
) -> Result<Option<CorniceRate>, sqlx::Error> {
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

fn seconds_from_event_rows(
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
