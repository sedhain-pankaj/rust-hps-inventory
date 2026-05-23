use std::{
    fs,
    path::{Path, PathBuf},
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
const LEGACY_ADMIN_HASH: &str = "74327943f791e17b6081b590be47d518d885b79972d37087df480448e0672094";

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

        let fingerprint_dir = data_dir.join("fingerprints");
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
        seed_assets(&db).await?;
        seed_if_needed(&db, &paths).await?;

        Ok(Self { db, paths })
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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cornice_rates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            series TEXT NOT NULL,
            model TEXT NOT NULL,
            unit_text TEXT NOT NULL,
            unit_value REAL,
            is_confidential INTEGER NOT NULL DEFAULT 1,
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
            unit_text TEXT NOT NULL DEFAULT '',
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
            let unit_text = row.get(index + 1).map(clean_cell).unwrap_or_default();
            if !series.is_empty() && !model.is_empty() {
                sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO cornice_rates
                        (series, model, unit_text, unit_value, is_confidential, updated_at)
                    VALUES (?, ?, ?, ?, 1, ?)
                    "#,
                )
                .bind(series)
                .bind(model)
                .bind(&unit_text)
                .bind(first_number(&unit_text))
                .bind(&now)
                .execute(db)
                .await?;
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
        let model = row.get(0).map(clean_cell).unwrap_or_default();
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
        let id = row.get(0).map(clean_cell).unwrap_or_default();
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
        let timestamp = row.get(0).map(clean_cell).unwrap_or_default();
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
               EXISTS(SELECT 1 FROM fingerprint_templates f WHERE f.employee_id = e.id) AS has_fingerprint
        FROM employees e
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
               EXISTS(SELECT 1 FROM fingerprint_templates f WHERE f.employee_id = e.id) AS has_fingerprint
        FROM employees e
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
    clean_cell(value)
        .replace('\u{201c}', "")
        .replace('\u{201d}', "")
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
