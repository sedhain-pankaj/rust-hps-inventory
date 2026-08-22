use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use anyhow::{Context, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Datelike, Local, NaiveDate};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use tauri::Manager;

use crate::models::*;

const CORNICE_RATE_CSV: &str = include_str!("../../assets/cornice_rate.csv");
const OVERALL_STOCK_CSV: &str = include_str!("../../assets/overall_stock.csv");
const HPS_LOGO: &[u8] = include_bytes!("../../assets/HPS.png");
const LEGACY_ADMIN_HASH: &str = "8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918";

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

        Ok(Self {
            db,
            paths,
            fingerprint_progress: Arc::new(Mutex::new(Vec::new())),
            enroll_jobs: Arc::new(Mutex::new(HashMap::new())),
            enroll_job_seq: Arc::new(AtomicU64::new(0)),
            auth_jobs: Arc::new(Mutex::new(HashMap::new())),
            auth_job_seq: Arc::new(AtomicU64::new(0)),
            active_helper_pids: Arc::new(Mutex::new(HashSet::new())),
        })
    }
}

pub async fn migrate(db: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS employees (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            finger TEXT NOT NULL DEFAULT 'right-index',
            active INTEGER NOT NULL DEFAULT 1,
            is_admin INTEGER NOT NULL DEFAULT 0,
            password_hash TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS employee_permissions (
            employee_id TEXT NOT NULL,
            permission TEXT NOT NULL,
            PRIMARY KEY (employee_id, permission),
            FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(db)
    .await?;

    // Migrate fingerprint_templates from the legacy multi-template schema
    // (composite unique key with template_index) back to a single template per
    // employee (employee_id primary key). Keeps each employee's designated
    // finger at the lowest template_index.
    let has_template_index: bool = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) > 0 FROM pragma_table_info('fingerprint_templates')
        WHERE name = 'template_index'
        "#,
    )
    .fetch_one(db)
    .await
    .unwrap_or(false);

    if has_template_index {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS fingerprint_templates_new (
                employee_id TEXT PRIMARY KEY,
                finger TEXT NOT NULL,
                template BLOB NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(db)
        .await?;

        // Keep the designated-finger template at the lowest template_index per employee
        let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fingerprint_templates")
            .fetch_one(db)
            .await
            .unwrap_or(0);
        if row_count > 0 {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO fingerprint_templates_new
                    (employee_id, finger, template, updated_at)
                SELECT t.employee_id, t.finger, t.template, t.updated_at
                FROM fingerprint_templates t
                JOIN employees e ON e.id = t.employee_id
                WHERE t.finger = e.finger
                  AND t.template_index = (
                      SELECT MIN(t2.template_index)
                      FROM fingerprint_templates t2
                      WHERE t2.employee_id = t.employee_id AND t2.finger = e.finger
                  )
                "#,
            )
            .execute(db)
            .await?;
        }

        sqlx::query("DROP TABLE fingerprint_templates")
            .execute(db)
            .await?;
        sqlx::query("ALTER TABLE fingerprint_templates_new RENAME TO fingerprint_templates")
            .execute(db)
            .await?;
    } else {
        // Fresh install or already single-template — ensure the table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS fingerprint_templates (
                employee_id TEXT PRIMARY KEY,
                finger TEXT NOT NULL,
                template BLOB NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(db)
        .await?;
    }

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cornice_rates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            series TEXT NOT NULL,
            model TEXT NOT NULL,
            unit TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE (series, model)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS stock_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item_type TEXT NOT NULL DEFAULT 'cornice',
            model TEXT NOT NULL,
            stock INTEGER NOT NULL DEFAULT 0,
            location TEXT NOT NULL DEFAULT '',
            dimensions TEXT NOT NULL DEFAULT '',
            photo_path TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            UNIQUE (item_type, model)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS time_clock_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id TEXT NOT NULL,
            work_date TEXT NOT NULL,
            action TEXT NOT NULL CHECK (action IN ('clock_in', 'clock_out')),
            timestamp TEXT NOT NULL,
            source TEXT NOT NULL,
            needs_admin_review INTEGER NOT NULL DEFAULT 0,
            note TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (employee_id) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cornice_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id TEXT NOT NULL,
            log_date TEXT NOT NULL,
            week_start TEXT NOT NULL,
            series TEXT NOT NULL,
            model TEXT NOT NULL,
            lengths INTEGER NOT NULL,
            unit TEXT NOT NULL DEFAULT '',
            unit_value REAL,
            total_units REAL NOT NULL DEFAULT 0,
            is_custom INTEGER NOT NULL DEFAULT 0,
            needs_admin_review INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            FOREIGN KEY (employee_id) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS production_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id TEXT NOT NULL,
            log_date TEXT NOT NULL,
            item TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            FOREIGN KEY (employee_id) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS overstock_locations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            aisle TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            updated_by TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (updated_by) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS deliveries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            driver_id TEXT NOT NULL,
            delivery_date TEXT NOT NULL,
            address TEXT NOT NULL,
            items TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            FOREIGN KEY (driver_id) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS admin_notifications (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            severity TEXT NOT NULL,
            kind TEXT NOT NULL,
            message TEXT NOT NULL,
            entity_table TEXT NOT NULL DEFAULT '',
            entity_id INTEGER,
            resolved INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_assets (
            key TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            media_type TEXT NOT NULL,
            content BLOB NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    // Mould inventory — which mould, where it's stored. Read-only for all staff, editable by storekeeper/admin.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mould_inventory (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            mould_name TEXT NOT NULL,
            storage_location TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            UNIQUE (mould_name)
        );
        "#,
    )
    .execute(db)
    .await?;

    // Cornice stock — actual castings: which cornice, aisle, in-stock qty, reserved qty.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cornice_stock (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model TEXT NOT NULL,
            aisle TEXT NOT NULL DEFAULT '',
            quantity_in_stock INTEGER NOT NULL DEFAULT 0,
            quantity_reserved INTEGER NOT NULL DEFAULT 0,
            remarks TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            UNIQUE (model)
        );
        "#,
    )
    .execute(db)
    .await?;

    // Dispatch orders — admin creates, driver views and logs against them.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS dispatch_orders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            cornice_model TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            delivery_location TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'in_progress', 'delivered')),
            created_by TEXT NOT NULL,
            delivered_by TEXT,
            delivered_at TEXT,
            remarks TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            FOREIGN KEY (created_by) REFERENCES employees(id),
            FOREIGN KEY (delivered_by) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    // Clock event edit audit trail.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS clock_event_edits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id INTEGER NOT NULL,
            edited_by TEXT NOT NULL,
            field_name TEXT NOT NULL,
            old_value TEXT NOT NULL,
            new_value TEXT NOT NULL,
            reason TEXT NOT NULL DEFAULT '',
            edited_at TEXT NOT NULL,
            FOREIGN KEY (event_id) REFERENCES time_clock_events(id),
            FOREIGN KEY (edited_by) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    // Payroll periods — weekly pay records per employee.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS payroll_periods (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id TEXT NOT NULL,
            week_start TEXT NOT NULL,
            week_end TEXT NOT NULL,
            total_hours REAL NOT NULL DEFAULT 0,
            total_units_known REAL NOT NULL DEFAULT 0,
            unit_threshold REAL NOT NULL DEFAULT 180,
            base_pay REAL NOT NULL DEFAULT 1140.0,
            extra_unit_pay REAL NOT NULL DEFAULT 0.0,
            gross_pay REAL NOT NULL DEFAULT 0.0,
            status TEXT NOT NULL DEFAULT 'pending',
            unknown_rate_equation TEXT NOT NULL DEFAULT '',
            needs_admin_review INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            UNIQUE (employee_id, week_start),
            FOREIGN KEY (employee_id) REFERENCES employees(id)
        );
        "#,
    )
    .execute(db)
    .await?;

    // Add staff_category column to employees if it doesn't exist.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _schema_migration_log (
            migration_id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn run_column_migrations(db: &SqlitePool) -> Result<()> {
    let migrations = [
        (
            "add_staff_category_to_employees",
            r#"ALTER TABLE employees ADD COLUMN staff_category TEXT NOT NULL DEFAULT 'cornice_hand'"#,
        ),
    ];

    for (migration_id, sql) in migrations {
        let applied: Option<String> = sqlx::query(
            "SELECT migration_id FROM _schema_migration_log WHERE migration_id = ?",
        )
        .bind(migration_id)
        .fetch_optional(db)
        .await?
        .map(|row| row.get::<String, _>("migration_id"));

        if applied.is_none() {
            // Check if the column already exists by trying a query
            let exists: Option<i64> = sqlx::query(
                "SELECT COUNT(*) as cnt FROM pragma_table_info('employees') WHERE name = 'staff_category'",
            )
            .fetch_one(db)
            .await?
            .get("cnt");

            if exists == Some(0) {
                sqlx::query(sql).execute(db).await?;
                sqlx::query(
                    "INSERT OR REPLACE INTO _schema_migration_log (migration_id, applied_at) VALUES (?, ?)",
                )
                .bind(migration_id)
                .bind(now_string())
                .execute(db)
                .await?;
            } else {
                // Column exists but migration wasn't logged
                sqlx::query(
                    "INSERT OR REPLACE INTO _schema_migration_log (migration_id, applied_at) VALUES (?, ?)",
                )
                .bind(migration_id)
                .bind(now_string())
                .execute(db)
                .await?;
            }
        }
    }

    Ok(())
}

async fn run_data_migrations(db: &SqlitePool) -> Result<()> {
    // 1. stock_items.reserved (for the merged cornice stock)
    alter_if_missing(
        db,
        "add_reserved_to_stock_items",
        "stock_items",
        "reserved",
        "ALTER TABLE stock_items ADD COLUMN reserved INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    // 2. cornice_logs amendment tracking
    alter_if_missing(
        db,
        "add_prev_values_to_cornice_logs",
        "cornice_logs",
        "prev_values",
        "ALTER TABLE cornice_logs ADD COLUMN prev_values TEXT",
    )
    .await?;
    alter_if_missing(
        db,
        "add_amended_at_to_cornice_logs",
        "cornice_logs",
        "amended_at",
        "ALTER TABLE cornice_logs ADD COLUMN amended_at TEXT",
    )
    .await?;
    alter_if_missing(
        db,
        "add_amended_by_to_cornice_logs",
        "cornice_logs",
        "amended_by",
        "ALTER TABLE cornice_logs ADD COLUMN amended_by TEXT",
    )
    .await?;

    // 3. mould_locations table + fixed seed locations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mould_locations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            sort_order INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .execute(db)
    .await?;
    log_migration(db, "create_mould_locations").await?;
    for (index, name) in ["Near Dryer", "Singles Wall", "Doubles Wall"].iter().enumerate() {
        sqlx::query("INSERT OR IGNORE INTO mould_locations (name, sort_order) VALUES (?, ?)")
            .bind(name)
            .bind(index as i64)
            .execute(db)
            .await?;
    }

    // 4. Copy cornice_stock rows into stock_items. Idempotent; runs on every
    //    startup (never logged) so late-arriving legacy rows are still copied.
    sqlx::query(
        r#"
        INSERT INTO stock_items (item_type, model, stock, location, reserved, notes, updated_at)
        SELECT 'cornice', c.model, c.quantity_in_stock, c.aisle, c.quantity_reserved, c.remarks, ?
        FROM cornice_stock c
        WHERE NOT EXISTS (
            SELECT 1 FROM stock_items s WHERE s.item_type = 'cornice' AND s.model = c.model
        )
        "#,
    )
    .bind(now_string())
    .execute(db)
    .await?;

    Ok(())
}

pub async fn column_exists(db: &SqlitePool, table: &str, column: &str) -> bool {
    let cnt: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
    ))
    .fetch_one(db)
    .await
    .unwrap_or(0);
    cnt > 0
}

async fn split_ambiguous_cornice_units(db: &SqlitePool) -> Result<()> {
    let rows = sqlx::query(
        "SELECT id, series, model, unit FROM cornice_rates
         WHERE unit LIKE '% or %' AND model NOT LIKE '% (ambg %'",
    )
    .fetch_all(db)
    .await?;
    let now = now_string();
    for row in rows {
        let id: i64 = row.get("id");
        let series: String = row.get("series");
        let model: String = row.get("model");
        let unit: String = row.get("unit");
        let expanded = expand_ambiguous(&model, &unit);
        if expanded.len() <= 1 {
            continue;
        }
        sqlx::query("UPDATE cornice_rates SET model = ?, unit = ? WHERE id = ?")
            .bind(&expanded[0].0)
            .bind(&expanded[0].1)
            .bind(id)
            .execute(db)
            .await?;
        for (m, u) in &expanded[1..] {
            sqlx::query(
                "INSERT OR IGNORE INTO cornice_rates (series, model, unit, updated_at) VALUES (?, ?, ?, ?)",
            )
            .bind(&series)
            .bind(m)
            .bind(u)
            .bind(&now)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

pub async fn run_cornice_unit_migrations(db: &SqlitePool) -> Result<()> {
    sqlx::query("DROP TABLE IF EXISTS cornice_rate_values").execute(db).await?;
    sqlx::query("DROP TABLE IF EXISTS cornice_series").execute(db).await?;
    if column_exists(db, "cornice_rates", "is_confidential").await {
        sqlx::query("ALTER TABLE cornice_rates DROP COLUMN is_confidential").execute(db).await?;
    }
    if column_exists(db, "cornice_rates", "unit_text").await
        && !column_exists(db, "cornice_rates", "unit").await
    {
        sqlx::query("ALTER TABLE cornice_rates RENAME COLUMN unit_text TO unit")
            .execute(db)
            .await?;
    }
    if column_exists(db, "cornice_rates", "unit_value").await {
        sqlx::query("ALTER TABLE cornice_rates DROP COLUMN unit_value").execute(db).await?;
    }
    split_ambiguous_cornice_units(db).await?;
    sqlx::query("UPDATE cornice_rates SET unit = 'Unknown' WHERE trim(unit) = '??'")
        .execute(db)
        .await?;
    if column_exists(db, "cornice_logs", "unit_text").await
        && !column_exists(db, "cornice_logs", "unit").await
    {
        sqlx::query("ALTER TABLE cornice_logs RENAME COLUMN unit_text TO unit")
            .execute(db)
            .await?;
    }
    Ok(())
}

async fn alter_if_missing(
    db: &SqlitePool,
    migration_id: &str,
    table: &str,
    guard_column: &str,
    sql: &str,
) -> Result<()> {
    log_migration_if_unapplied(db, migration_id).await?;
    let exists: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{guard_column}'"
    ))
    .fetch_one(db)
    .await?;
    if exists == 0 {
        sqlx::query(sql).execute(db).await?;
    }
    Ok(())
}

async fn log_migration_if_unapplied(db: &SqlitePool, migration_id: &str) -> Result<bool> {
    let applied: Option<String> = sqlx::query(
        "SELECT migration_id FROM _schema_migration_log WHERE migration_id = ?",
    )
    .bind(migration_id)
    .fetch_optional(db)
    .await?
    .map(|row| row.get::<String, _>("migration_id"));
    if applied.is_none() {
        sqlx::query(
            "INSERT OR REPLACE INTO _schema_migration_log (migration_id, applied_at) VALUES (?, ?)",
        )
        .bind(migration_id)
        .bind(now_string())
        .execute(db)
        .await?;
    }
    Ok(applied.is_none())
}

async fn log_migration(db: &SqlitePool, migration_id: &str) -> Result<()> {
    log_migration_if_unapplied(db, migration_id).await?;
    Ok(())
}

async fn seed_assets(db: &SqlitePool) -> Result<()> {
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

async fn seed_if_needed(db: &SqlitePool, paths: &AppPaths) -> Result<()> {
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
               f.updated_at AS fingerprint_updated_at
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
               f.updated_at AS fingerprint_updated_at
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

pub fn now_string() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn today_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn parse_date_or_today(value: Option<String>) -> NaiveDate {
    value
        .and_then(|text| NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Local::now().date_naive())
}

pub fn week_start_for(date: NaiveDate) -> NaiveDate {
    let weekday = date.weekday().num_days_from_monday() as i64;
    let wednesday = 2_i64;
    let delta = (weekday + 7 - wednesday) % 7;
    date - chrono::Duration::days(delta)
}

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    if let Ok(hash) = Argon2::default().hash_password(password.as_bytes(), &salt) {
        return hash.to_string();
    }
    legacy_sha256(password)
}

pub fn verify_password(stored_hash: &str, password: &str) -> bool {
    if stored_hash.starts_with("$argon2") {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return false;
        };
        return Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
    }
    stored_hash == legacy_sha256(password)
}

pub fn is_legacy_password_hash(stored_hash: &str) -> bool {
    !stored_hash.starts_with("$argon2")
}

fn legacy_sha256(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    to_hex(&hasher.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub fn format_seconds(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format!("{hours:02}:{minutes:02}")
}

pub fn clean_cell(value: impl AsRef<str>) -> String {
    value.as_ref().trim().trim_matches('"').trim().to_string()
}

fn clean_series(value: String) -> String {
    clean_cell(value).replace(['\u{201c}', '\u{201d}'], "")
}

pub fn normalize_unit(unit: &str) -> String {
    let t = unit.trim();
    if t.is_empty() || t == "??" {
        "Unknown".to_string()
    } else {
        t.to_string()
    }
}

pub fn expand_ambiguous(model: &str, unit: &str) -> Vec<(String, String)> {
    let parts: Vec<String> = unit
        .split(" or ")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() <= 1 {
        vec![(model.to_string(), normalize_unit(unit))]
    } else {
        parts
            .iter()
            .enumerate()
            .map(|(i, p)| (format!("{} (ambg {})", model, i + 1), normalize_unit(p)))
            .collect()
    }
}

pub fn unit_value(unit: &str) -> Option<f64> {
    let t = unit.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("unknown") {
        return None;
    }
    if let Some((a, b)) = t.split_once('/') {
        if let (Ok(num), Ok(den)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            if den != 0.0 {
                return Some(num / den);
            }
        }
    }
    if let Ok(n) = t.parse::<f64>() {
        return Some(n);
    }
    first_number(t)
}

fn first_number(value: &str) -> Option<f64> {
    let mut started = false;
    let mut number = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || (ch == '.' && started) {
            started = true;
            number.push(ch);
        } else if started {
            break;
        }
    }
    number.parse::<f64>().ok()
}

fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = content.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                row.push(cell.trim().to_string());
                cell.clear();
            }
            '\n' if !in_quotes => {
                row.push(cell.trim_end_matches('\r').trim().to_string());
                cell.clear();
                if row.iter().any(|value| !value.is_empty()) {
                    rows.push(row);
                }
                row = Vec::new();
            }
            _ => cell.push(ch),
        }
    }

    if !cell.is_empty() || !row.is_empty() {
        row.push(cell.trim_end_matches('\r').trim().to_string());
        if row.iter().any(|value| !value.is_empty()) {
            rows.push(row);
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn migrations_are_idempotent_and_add_new_columns() {
        let pool = fresh_pool().await;
        run_all_migrations(&pool).await;
        run_all_migrations(&pool).await;

        let reserved: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('stock_items') WHERE name = 'reserved'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(reserved, 1);

        for column in ["prev_values", "amended_at", "amended_by"] {
            let count: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM pragma_table_info('cornice_logs') WHERE name = '{column}'"
            ))
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, 1, "column {column} missing");
        }

        let locations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM mould_locations").fetch_one(&pool).await.unwrap();
        assert_eq!(locations, 3);
        let first: String = sqlx::query_scalar(
            "SELECT name FROM mould_locations ORDER BY sort_order LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(first, "Near Dryer");
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

    #[test]
    fn normalize_unit_converts_question_marks() {
        assert_eq!(normalize_unit("??"), "Unknown");
        assert_eq!(normalize_unit("  ??  "), "Unknown");
        assert_eq!(normalize_unit(""), "Unknown");
        assert_eq!(normalize_unit("1.5"), "1.5");
    }

    #[test]
    fn expand_ambiguous_splits_or_units() {
        assert_eq!(
            expand_ambiguous("491", "1.2 or 2"),
            vec![
                ("491 (ambg 1)".to_string(), "1.2".to_string()),
                ("491 (ambg 2)".to_string(), "2".to_string()),
            ]
        );
        assert_eq!(
            expand_ambiguous("404", "1.5"),
            vec![("404".to_string(), "1.5".to_string())]
        );
        assert_eq!(
            expand_ambiguous("722", "??"),
            vec![("722".to_string(), "Unknown".to_string())]
        );
        assert_eq!(
            expand_ambiguous("909", "2 or 15/6"),
            vec![
                ("909 (ambg 1)".to_string(), "2".to_string()),
                ("909 (ambg 2)".to_string(), "15/6".to_string()),
            ]
        );
    }

    #[test]
    fn unit_value_parses_numbers_fractions_and_unknown() {
        assert_eq!(unit_value("1.25"), Some(1.25));
        assert_eq!(unit_value("2"), Some(2.0));
        assert_eq!(unit_value("15/6"), Some(2.5));
        assert_eq!(unit_value("Unknown"), None);
        assert_eq!(unit_value("??"), None);
        assert_eq!(unit_value(""), None);
    }

    #[tokio::test]
    async fn cornice_unit_migration_splits_renames_and_drops() {
        let pool = fresh_pool().await;
        sqlx::query(
            "CREATE TABLE cornice_rates (
                id INTEGER PRIMARY KEY AUTOINCREMENT, series TEXT NOT NULL, model TEXT NOT NULL,
                unit_text TEXT NOT NULL, unit_value REAL, is_confidential INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT NOT NULL, UNIQUE (series, model))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE cornice_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT, employee_id TEXT NOT NULL, log_date TEXT NOT NULL,
                week_start TEXT NOT NULL, series TEXT NOT NULL, model TEXT NOT NULL, lengths INTEGER NOT NULL,
                unit_text TEXT NOT NULL DEFAULT '', unit_value REAL, total_units REAL NOT NULL DEFAULT 0,
                is_custom INTEGER NOT NULL DEFAULT 0, needs_admin_review INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cornice_rates (series, model, unit_text, unit_value, is_confidential, updated_at)
             VALUES ('S','491','1.2 or 2',1.2,1,'2026-01-01T00:00:00'),
                    ('S','722','??',NULL,1,'2026-01-01T00:00:00'),
                    ('S','404','1.5',1.5,1,'2026-01-01T00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        run_cornice_unit_migrations(&pool).await.unwrap();
        run_cornice_unit_migrations(&pool).await.unwrap();

        assert!(column_exists(&pool, "cornice_rates", "unit").await);
        assert!(!column_exists(&pool, "cornice_rates", "unit_text").await);
        assert!(!column_exists(&pool, "cornice_rates", "unit_value").await);
        assert!(!column_exists(&pool, "cornice_rates", "is_confidential").await);
        assert!(column_exists(&pool, "cornice_logs", "unit").await);
        assert!(!column_exists(&pool, "cornice_logs", "unit_text").await);

        let models: Vec<String> =
            sqlx::query_scalar("SELECT model FROM cornice_rates WHERE series='S' ORDER BY model")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(models.contains(&"491 (ambg 1)".to_string()));
        assert!(models.contains(&"491 (ambg 2)".to_string()));
        assert!(!models.contains(&"491".to_string()));

        let u722: String = sqlx::query_scalar("SELECT unit FROM cornice_rates WHERE model='722'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(u722, "Unknown");
        let u404: String = sqlx::query_scalar("SELECT unit FROM cornice_rates WHERE model='404'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(u404, "1.5");
    }

    #[tokio::test]
    async fn fingerprint_migration_converts_multi_template_to_single() {
        let pool = fresh_pool().await;
        // Establish the base schema (single-template fingerprint_templates + employees).
        migrate(&pool).await.unwrap();

        // Simulate the legacy multi-template state: one employee with 3 templates on the
        // designated finger plus 1 on a different finger.
        sqlx::query(
            "INSERT INTO employees (id, name, finger, active, is_admin, password_hash, created_at, updated_at)
             VALUES ('E1','One','right-index',1,0,'x','2026-01-01T00:00:00','2026-01-01T00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DROP TABLE fingerprint_templates")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE fingerprint_templates (
                id INTEGER PRIMARY KEY AUTOINCREMENT, employee_id TEXT NOT NULL, finger TEXT NOT NULL,
                template_index INTEGER NOT NULL DEFAULT 1, template BLOB NOT NULL, updated_at TEXT NOT NULL,
                UNIQUE (employee_id, finger, template_index),
                FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO fingerprint_templates (employee_id, finger, template_index, template, updated_at)
             VALUES ('E1','right-index',1,X'01','2026-01-01T00:00:00'),
                    ('E1','right-index',2,X'02','2026-01-01T00:00:00'),
                    ('E1','right-index',3,X'03','2026-01-01T00:00:00'),
                    ('E1','left-index',1,X'04','2026-01-01T00:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Re-run migrate(): the fingerprint migration collapses to one row per employee,
        // keeping the designated finger at the lowest template_index. Idempotent on re-run.
        migrate(&pool).await.unwrap();
        migrate(&pool).await.unwrap();

        assert!(!column_exists(&pool, "fingerprint_templates", "template_index").await);
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT employee_id, finger FROM fingerprint_templates")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![("E1".to_string(), "right-index".to_string())]);
        let tpl: Vec<u8> = sqlx::query_scalar(
            "SELECT template FROM fingerprint_templates WHERE employee_id='E1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tpl, vec![0x01]);
    }
}
