use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub database_path: String,
    pub fingerprint_helper_found: bool,
    pub fingerprint_helper_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Employee {
    pub id: String,
    pub name: String,
    pub finger: String,
    pub active: bool,
    pub is_admin: bool,
    pub has_password: bool,
    pub has_fingerprint: bool,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmployeeInput {
    pub id: String,
    pub name: String,
    pub finger: String,
    pub active: bool,
    pub is_admin: bool,
    pub password: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthResponse {
    pub employee: Employee,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FingerprintEnrollResponse {
    pub employee: Employee,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StockItem {
    pub id: i64,
    pub item_type: String,
    pub model: String,
    pub stock: i64,
    pub location: String,
    pub dimensions: String,
    pub photo_path: String,
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StockItemInput {
    pub id: Option<i64>,
    pub item_type: String,
    pub model: String,
    pub stock: i64,
    pub location: String,
    pub dimensions: String,
    pub photo_path: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorniceRate {
    pub id: i64,
    pub series: String,
    pub model: String,
    pub unit_text: String,
    pub unit_value: Option<f64>,
    pub is_confidential: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CorniceRateInput {
    pub id: Option<i64>,
    pub series: String,
    pub model: String,
    pub unit_text: String,
    pub unit_value: Option<f64>,
    pub is_confidential: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClockRequest {
    pub employee_id: String,
    pub action: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClockEvent {
    pub id: i64,
    pub employee_id: String,
    pub employee_name: String,
    pub work_date: String,
    pub action: String,
    pub timestamp: String,
    pub source: String,
    pub needs_admin_review: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttendanceSummary {
    pub employee_id: String,
    pub employee_name: String,
    pub work_date: String,
    pub hours: String,
    pub seconds: i64,
    pub status: String,
    pub needs_admin_review: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminAlert {
    pub id: i64,
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub entity_table: String,
    pub entity_id: Option<i64>,
    pub resolved: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CorniceLogInput {
    pub employee_id: String,
    pub log_date: Option<String>,
    pub series: String,
    pub model: String,
    pub lengths: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorniceLog {
    pub id: i64,
    pub employee_id: String,
    pub employee_name: String,
    pub log_date: String,
    pub week_start: String,
    pub series: String,
    pub model: String,
    pub lengths: i64,
    pub unit_text: String,
    pub unit_value: Option<f64>,
    pub total_units: f64,
    pub weekly_units: f64,
    pub is_custom: bool,
    pub needs_admin_review: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductionLogInput {
    pub employee_id: String,
    pub log_date: Option<String>,
    pub item: String,
    pub quantity: i64,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductionLog {
    pub id: i64,
    pub employee_id: String,
    pub employee_name: String,
    pub log_date: String,
    pub item: String,
    pub quantity: i64,
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverstockInput {
    pub employee_id: String,
    pub model: String,
    pub quantity: i64,
    pub aisle: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverstockItem {
    pub id: i64,
    pub model: String,
    pub quantity: i64,
    pub aisle: String,
    pub notes: String,
    pub updated_by: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeliveryInput {
    pub driver_id: String,
    pub delivery_date: Option<String>,
    pub address: String,
    pub items: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Delivery {
    pub id: i64,
    pub driver_id: String,
    pub driver_name: String,
    pub delivery_date: String,
    pub address: String,
    pub items: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminTableInfo {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminColumnInfo {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub editable: bool,
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminTableRow {
    pub rowid: i64,
    pub values: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminTableData {
    pub table: String,
    pub label: String,
    pub columns: Vec<AdminColumnInfo>,
    pub rows: Vec<AdminTableRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminTableSaveInput {
    pub table: String,
    pub rowid: Option<i64>,
    pub values: Value,
}
