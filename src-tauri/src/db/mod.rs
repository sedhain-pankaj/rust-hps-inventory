use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use tauri::Manager;

use crate::models::*;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub fingerprint_dir: PathBuf,
    pub resource_dir: Option<PathBuf>,
    pub source_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub paths: AppPaths,
    pub fingerprint_progress: Arc<Mutex<Vec<String>>>,
    pub enroll_jobs: Arc<Mutex<HashMap<String, FingerprintEnrollJob>>>,
    pub enroll_job_seq: Arc<AtomicU64>,
    pub auth_jobs: Arc<Mutex<HashMap<String, FingerprintAuthJob>>>,
    pub auth_job_seq: Arc<AtomicU64>,
    pub active_helper_pids: Arc<Mutex<HashSet<u32>>>,
    /// Set by the fingerprint-gated `exit_kiosk` command so the
    /// `ExitRequested` handler allows the process to actually terminate.
    pub allow_exit: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct FingerprintEnrollJob {
    pub employee_id: String,
    pub lines: Vec<String>,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FingerprintAuthJob {
    pub matched_id: Option<String>,
    pub lines: Vec<String>,
    pub done: bool,
    pub error: Option<String>,
}

impl AppState {
    pub fn next_enroll_job_id(&self) -> String {
        let id = self.enroll_job_seq.fetch_add(1, Ordering::Relaxed) + 1;
        format!("enroll-{id}")
    }

    pub fn next_auth_job_id(&self) -> String {
        let id = self.auth_job_seq.fetch_add(1, Ordering::Relaxed) + 1;
        format!("auth-{id}")
    }
}

impl AppState {
    pub async fn initialize(app: &tauri::AppHandle) -> Result<Self> {
        let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        // Keep fingerprint and other runtime data in the OS application data dir,
        // but store the SQLite database in the project root as `hps.db`.
        let data_dir = app
            .path()
            .app_data_dir()
            .context("Could not resolve application data directory")?;
        fs::create_dir_all(&data_dir).context("Could not create application data directory")?;

        let fingerprint_dir = source_root.join("data").join("fingerprints");
        fs::create_dir_all(&fingerprint_dir).context("Could not create fingerprint directory")?;

        let db_path = source_root.join("hps.db");
        let connect_options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .pragma("foreign_keys", "ON");

        let db = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .context("Could not open SQLite database at project root")?;

        let paths = AppPaths {
            data_dir,
            db_path,
            fingerprint_dir,
            resource_dir: app.path().resource_dir().ok(),
            source_root,
        };

        migrate(&db).await?;
        run_column_migrations(&db).await?;
        run_data_migrations(&db).await?;
        run_cornice_unit_migrations(&db).await?;
        seed_assets(&db).await?;
        seed_if_needed(&db, &paths).await?;
        backfill_mould_view_permissions(&db).await?;

        Ok(Self {
            db,
            paths,
            fingerprint_progress: Arc::new(Mutex::new(Vec::new())),
            enroll_jobs: Arc::new(Mutex::new(HashMap::new())),
            enroll_job_seq: Arc::new(AtomicU64::new(0)),
            auth_jobs: Arc::new(Mutex::new(HashMap::new())),
            auth_job_seq: Arc::new(AtomicU64::new(0)),
            active_helper_pids: Arc::new(Mutex::new(HashSet::new())),
            allow_exit: Arc::new(AtomicBool::new(false)),
        })
    }
}
pub async fn set_permissions(
    db: &SqlitePool,
    employee_id: &str,
    permissions: &[&str],
) -> Result<()> {
    sqlx::query("DELETE FROM employee_permissions WHERE employee_id = ?")
        .bind(employee_id)
        .execute(db)
        .await?;
    for permission in permissions {
        sqlx::query(
            "INSERT OR IGNORE INTO employee_permissions (employee_id, permission) VALUES (?, ?)",
        )
        .bind(employee_id)
        .bind(permission)
        .execute(db)
        .await?;
    }
    Ok(())
}

pub async fn permissions_for(
    db: &SqlitePool,
    employee_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT permission FROM employee_permissions WHERE employee_id = ? ORDER BY permission",
    )
    .bind(employee_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(|row| row.get("permission")).collect())
}

pub async fn employee_by_id(
    db: &SqlitePool,
    employee_id: &str,
) -> Result<Option<Employee>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT e.*,
               EXISTS(SELECT 1 FROM fingerprint_templates f WHERE f.employee_id = e.id) AS has_fingerprint,
               f.updated_at AS fingerprint_updated_at,
               f.finger AS template_finger
        FROM employees e
        LEFT JOIN fingerprint_templates f ON f.employee_id = e.id
        WHERE e.id = ?
        "#,
    )
    .bind(employee_id)
    .fetch_optional(db)
    .await?;

    match row {
        Some(row) => employee_from_row(db, row).await.map(Some),
        None => Ok(None),
    }
}

pub async fn list_employees(
    db: &SqlitePool,
    include_inactive: bool,
) -> Result<Vec<Employee>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT e.*,
               EXISTS(SELECT 1 FROM fingerprint_templates f WHERE f.employee_id = e.id) AS has_fingerprint,
               f.updated_at AS fingerprint_updated_at,
               f.finger AS template_finger
        FROM employees e
        LEFT JOIN fingerprint_templates f ON f.employee_id = e.id
        WHERE (? = 1 OR e.active = 1)
        ORDER BY e.name COLLATE NOCASE
        "#,
    )
    .bind(include_inactive as i64)
    .fetch_all(db)
    .await?;

    let mut employees = Vec::with_capacity(rows.len());
    for row in rows {
        employees.push(employee_from_row(db, row).await?);
    }
    Ok(employees)
}

async fn employee_from_row(
    db: &SqlitePool,
    row: sqlx::sqlite::SqliteRow,
) -> Result<Employee, sqlx::Error> {
    let id: String = row.get("id");
    Ok(Employee {
        permissions: permissions_for(db, &id).await?,
        id,
        name: row.get("name"),
        finger: row.get("finger"),
        template_finger: row.get("template_finger"),
        active: row.get::<i64, _>("active") != 0,
        is_admin: row.get::<i64, _>("is_admin") != 0,
        has_password: row.get::<Option<String>, _>("password_hash").is_some(),
        has_fingerprint: row.get::<i64, _>("has_fingerprint") != 0,
        fingerprint_updated_at: row.get::<Option<String>, _>("fingerprint_updated_at"),
        staff_category: row.try_get("staff_category").unwrap_or_else(|_| "cornice_hand".to_string()),
    })
}

pub async fn notification(
    db: &SqlitePool,
    severity: &str,
    kind: &str,
    message: &str,
    entity_table: &str,
    entity_id: Option<i64>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO admin_notifications
            (severity, kind, message, entity_table, entity_id, resolved, created_at)
        VALUES (?, ?, ?, ?, ?, 0, ?)
        "#,
    )
    .bind(severity)
    .bind(kind)
    .bind(message)
    .bind(entity_table)
    .bind(entity_id)
    .bind(now_string())
    .execute(db)
    .await?;
    Ok(())
}

pub mod migrate;
pub mod seed;
pub mod util;

pub use migrate::*;
pub(crate) use seed::*;
pub use util::*;

