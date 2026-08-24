use base64::{engine::general_purpose, Engine as _};
use serde_json::{Map, Value};

use sqlx::Row;
use tauri::State;

use crate::{
    db::{notification, AppState},
    fingerprint,
    models::*,
};

pub(crate) type CommandResult<T> = Result<T, String>;

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
    editable: bool,
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
    col("unit", "Unit", AdminColumnKind::Text),
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
    col("unit", "Unit", AdminColumnKind::Text),
    col("unit_value", "Unit Value", AdminColumnKind::Real),
    col("total_units", "Total Units", AdminColumnKind::Real),
    col("is_custom", "Custom", AdminColumnKind::Bool),
    col("needs_admin_review", "Review", AdminColumnKind::Bool),
    readonly_col("prev_values", "Previous Values", AdminColumnKind::Text),
    readonly_col("amended_at", "Amended At", AdminColumnKind::Text),
    readonly_col("amended_by", "Amended By", AdminColumnKind::Text),
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
        editable: true,
    },
    AdminTable {
        name: "employee_permissions",
        label: "Employee Permissions",
        columns: EMPLOYEE_PERMISSION_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "fingerprint_templates",
        label: "Fingerprint Templates",
        columns: FINGERPRINT_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "cornice_rates",
        label: "Cornice Rates",
        columns: CORNICE_RATE_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "stock_items",
        label: "Stock Items",
        columns: STOCK_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "time_clock_events",
        label: "Time Clock Events",
        columns: TIME_CLOCK_COLUMNS,
        editable: false,
    },
    AdminTable {
        name: "cornice_logs",
        label: "Cornice Logs",
        columns: CORNICE_LOG_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "production_logs",
        label: "Production Logs",
        columns: PRODUCTION_LOG_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "overstock_locations",
        label: "Overstock Locations",
        columns: OVERSTOCK_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "deliveries",
        label: "Deliveries",
        columns: DELIVERY_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "admin_notifications",
        label: "Admin Notifications",
        columns: NOTIFICATION_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "app_meta",
        label: "App Metadata",
        columns: APP_META_COLUMNS,
        editable: true,
    },
    AdminTable {
        name: "app_assets",
        label: "App Assets",
        columns: APP_ASSET_COLUMNS,
        editable: true,
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
        editable: config.editable,
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
    if !config.editable {
        return Err("This table is read-only. Use the dedicated panel to edit.".to_string());
    }
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
        let placeholders = std::iter::repeat_n("?", editable_columns.len())
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
    if !config.editable {
        return Err("This table is read-only.".to_string());
    }
    let sql = format!("DELETE FROM {} WHERE rowid = ?", config.name);
    sqlx::query(&sql)
        .bind(rowid)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    list_admin_table_rows(state, config.name.to_string()).await
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

pub(crate) fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn disk_usage(path: &std::path::Path) -> Option<(u64, u64, f64)> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let frsize = stats.f_frsize as u64;
    let total = stats.f_blocks as u64 * frsize;
    let free = stats.f_bavail as u64 * frsize;
    let pct = if stats.f_blocks > 0 {
        100.0 * (1.0 - stats.f_bavail as f64 / stats.f_blocks as f64)
    } else {
        0.0
    };
    Some((total, free, pct))
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < UNITS.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    format!(
        "{} {}",
        if value >= 100.0 || index == 0 {
            value as u64
        } else {
            (value * 10.0) as u64 / 10
        },
        UNITS[index]
    )
}

#[tauri::command]
pub async fn storage_status(state: State<'_, AppState>) -> CommandResult<StorageStatus> {
    let db_path = state.paths.db_path.clone();
    let db_size_bytes = std::fs::metadata(&db_path).map(|meta| meta.len()).unwrap_or(0);
    let dir = db_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    match disk_usage(dir) {
        Some((total, free, pct)) => {
            if pct > 90.0 {
                let existing: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM admin_notifications WHERE kind = 'disk_space' AND resolved = 0 LIMIT 1",
                )
                .fetch_optional(&state.db)
                .await
                .map_err(to_string)?;
                if existing.is_none() {
                    notification(
                        &state.db,
                        "red",
                        "disk_space",
                        &format!(
                            "Disk is {pct:.0}% full — only {} free. Free up space.",
                            human_bytes(free)
                        ),
                        "app_meta",
                        None,
                    )
                    .await
                    .map_err(to_string)?;
                }
            }
            Ok(StorageStatus {
                db_path: db_path.to_string_lossy().to_string(),
                db_size_bytes,
                disk_total_bytes: total,
                disk_free_bytes: free,
                disk_used_pct: pct,
            })
        }
        None => Ok(StorageStatus {
            db_path: db_path.to_string_lossy().to_string(),
            db_size_bytes,
            disk_total_bytes: 0,
            disk_free_bytes: 0,
            disk_used_pct: 0.0,
        }),
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;

    #[test]
    fn disk_usage_returns_sane_values() {
        let (total, free, pct) = disk_usage(std::path::Path::new("/")).expect("statvfs failed");
        assert!(total > 0);
        assert!(free <= total);
        assert!((0.0..=100.0).contains(&pct));
    }
}


mod alerts;
mod dispatch;
mod employees;
mod logs;
mod moulds;
mod payroll;
mod rates;
mod stock;

pub use alerts::*;
pub use dispatch::*;
pub use employees::*;
pub use logs::*;
pub use moulds::*;
pub use payroll::*;
pub use rates::*;
pub use stock::*;
