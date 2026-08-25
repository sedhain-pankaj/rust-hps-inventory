use anyhow::Result;
use sqlx::{Row, SqlitePool};


use super::seed::seed_default_mould_columns;
use super::util::{expand_ambiguous, now_string};


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
                images BLOB,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(db)
        .await?;
    }

    // Migrate: add the `images` column (sub-print image bundle used for the
    // alignment hint on failed identify) to pre-existing databases.
    let has_images_column: bool = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) > 0 FROM pragma_table_info('fingerprint_templates')
        WHERE name = 'images'
        "#,
    )
    .fetch_one(db)
    .await
    .unwrap_or(false);
    if !has_images_column {
        sqlx::query("ALTER TABLE fingerprint_templates ADD COLUMN images BLOB")
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
            unit_threshold REAL NOT NULL DEFAULT 0,
            base_pay REAL NOT NULL DEFAULT 0.0,
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

pub(crate) async fn run_column_migrations(db: &SqlitePool) -> Result<()> {
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

pub(crate) async fn run_data_migrations(db: &SqlitePool) -> Result<()> {
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

    // 2b. payroll_periods review flag: records that an admin has already approved
    //     the week so it is not re-flagged for review.
    alter_if_missing(
        db,
        "add_reviewed_to_payroll_periods",
        "payroll_periods",
        "reviewed",
        "ALTER TABLE payroll_periods ADD COLUMN reviewed INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    // 2c. Proration override: `use_standard_week`, when set, treats the week as a
    //     standard 40-hr week for BOTH base pay and the unit threshold (used when
    //     the clocked hours are wrong). Replaces an earlier, never-used
    //     `threshold_override` column — drop it if a prior build added it.
    let has_threshold_override: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('payroll_periods') WHERE name = 'threshold_override'",
    )
    .fetch_one(db)
    .await?;
    if has_threshold_override > 0 {
        sqlx::query("ALTER TABLE payroll_periods DROP COLUMN threshold_override")
            .execute(db)
            .await?;
    }
    alter_if_missing(
        db,
        "add_use_standard_week_to_payroll_periods",
        "payroll_periods",
        "use_standard_week",
        "ALTER TABLE payroll_periods ADD COLUMN use_standard_week INTEGER NOT NULL DEFAULT 0",
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
    for (index, name) in ["Singles Wall", "Doubles Wall", "Near Dryer"].iter().enumerate() {
        sqlx::query("INSERT OR IGNORE INTO mould_locations (name, sort_order) VALUES (?, ?)")
            .bind(name)
            .bind(index as i64)
            .execute(db)
            .await?;
    }

    // 3b. Mould location columns (sub-locations): location -> column -> mould.
    //     Reorders the seed locations (Singles Wall, Doubles Wall, Near Dryer),
    //     gives every location a first column, and attaches legacy moulds to it.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mould_location_columns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            location_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            UNIQUE (location_id, name),
            FOREIGN KEY (location_id) REFERENCES mould_locations(id)
        );
        "#,
    )
    .execute(db)
    .await?;
    log_migration(db, "create_mould_location_columns").await?;
    alter_if_missing(
        db,
        "add_column_id_to_mould_inventory",
        "mould_inventory",
        "column_id",
        "ALTER TABLE mould_inventory ADD COLUMN column_id INTEGER",
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE mould_locations
        SET sort_order = CASE name
            WHEN 'Singles Wall' THEN 0
            WHEN 'Doubles Wall' THEN 1
            WHEN 'Near Dryer' THEN 2
            ELSE sort_order
        END
        WHERE name IN ('Singles Wall', 'Doubles Wall', 'Near Dryer')
        "#,
    )
    .execute(db)
    .await?;
    seed_default_mould_columns(db).await?;
    sqlx::query(
        r#"
        UPDATE mould_inventory
        SET column_id = (
            SELECT c.id
            FROM mould_location_columns c
            JOIN mould_locations l ON l.id = c.location_id
            WHERE l.name = mould_inventory.storage_location
            ORDER BY c.sort_order, c.id
            LIMIT 1
        )
        WHERE column_id IS NULL
          AND EXISTS (
              SELECT 1 FROM mould_locations l WHERE l.name = mould_inventory.storage_location
          )
        "#,
    )
    .execute(db)
    .await?;

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

    // 5. Round float noise in legacy amount rows (written before payroll math
    //    rounded to cents). Idempotent; only touches rows that carry noise.
    sqlx::query(
        r#"
        UPDATE payroll_periods
        SET total_units_known = ROUND(total_units_known, 2),
            extra_unit_pay = ROUND(extra_unit_pay, 2),
            gross_pay = ROUND(gross_pay, 2)
        WHERE total_units_known != ROUND(total_units_known, 2)
           OR extra_unit_pay != ROUND(extra_unit_pay, 2)
           OR gross_pay != ROUND(gross_pay, 2)
        "#,
    )
    .execute(db)
    .await?;
    sqlx::query(
        r#"
        UPDATE cornice_logs
        SET total_units = ROUND(total_units, 2)
        WHERE total_units != ROUND(total_units, 2)
        "#,
    )
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

pub(crate) async fn log_migration_if_unapplied(db: &SqlitePool, migration_id: &str) -> Result<bool> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePool;

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
        assert_eq!(first, "Singles Wall");
        let last: String = sqlx::query_scalar(
            "SELECT name FROM mould_locations ORDER BY sort_order DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(last, "Near Dryer");
        let columns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mould_location_columns")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(columns, 15, "each seeded location gets R1..R5");
        let first_col: String = sqlx::query_scalar(
            "SELECT c.name FROM mould_location_columns c
             JOIN mould_locations l ON l.id = c.location_id
             WHERE l.name = 'Singles Wall' ORDER BY c.sort_order LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(first_col, "R1");
        let has_column_id: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('mould_inventory') WHERE name = 'column_id'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(has_column_id, 1, "mould_inventory.column_id missing");
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
