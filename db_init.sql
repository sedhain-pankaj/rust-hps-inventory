PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

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

CREATE TABLE IF NOT EXISTS employee_permissions (
    employee_id TEXT NOT NULL,
    permission TEXT NOT NULL,
    PRIMARY KEY (employee_id, permission),
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS fingerprint_templates (
    employee_id TEXT PRIMARY KEY,
    finger TEXT NOT NULL,
    template BLOB NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (employee_id) REFERENCES employees(id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS app_assets (
    key TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    content BLOB NOT NULL,
    updated_at TEXT NOT NULL
);
