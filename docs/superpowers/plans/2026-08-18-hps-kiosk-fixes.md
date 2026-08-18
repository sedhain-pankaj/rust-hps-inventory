# HPS Kiosk Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved kiosk fixes (spec: `docs/superpowers/specs/2026-08-18-hps-kiosk-fixes-design.md`): timer/fingerprint visual polish, partial re-render on tab click, cornice-stock merge into STOCKS, extensible Mould Locations, grouped Rates, a shared inline-table editing component (core batch: Stocks, Mould Locations, Rates, staff cornice log), DB size + disk usage display with >90% alert, staff entry edit/delete with old→new pending approval, tabular week/day staff log, and clock in/out confirm modals without re-auth.

**Architecture:** The Rust/Tauri backend (`src-tauri/src/`) gets idempotent SQLite migrations (new columns + `mould_locations` table + data copy) and new commands (`storage_status`, mould-location CRUD, `delete_*` for stock/rate/mould, cornice-log update/delete/approve, `get_clock_status`). The vanilla-JS UI (`ui/`) gets a new shared inline-table component (`ui/js/table.js`) consumed by the core-batch panels, plus targeted fixes to `app.js`, `auth.js`, `icons.js`, `styles.css`, and one SVG asset.

**Tech Stack:** Rust (tauri 2.11, sqlx 0.7 sqlite, chrono, libc — **no new dependencies**), vanilla ES-module JS (no build step, `frontendDist: ../ui`), plain CSS.

## Global Constraints

- No new Rust or JS dependencies. `libc` is already a dependency (used for `statvfs`).
- All schema migrations are idempotent and recorded in `_schema_migration_log`. Exception: the `cornice_stock` → `stock_items` data copy runs on every startup (idempotent `INSERT ... WHERE NOT EXISTS`, never logged).
- The legacy `cornice_stock` table is **never dropped**.
- Backend commands return `CommandResult<T>` = `Result<T, String>` with user-facing error strings.
- Tauri maps JS camelCase invoke args to Rust snake_case params automatically (e.g. `actorId` → `actor_id`).
- UI: no framework. All user data rendered via `escapeHtml()`; raw-HTML cells only through the inline component's `cellHtml` hook or the existing `cellLooksHtml` prefixes.
- Do NOT touch fingerprint matching parameters (`score_threshold`, `distance_match`) or the helper protocol. A known fingerprint cross-match bug is out of scope (see spec §6).
- Verify backend with `cd src-tauri && cargo check && cargo test`. Verify JS syntax with `node --check --input-type=module < ui/js/<file>.js` (run from repo root).
- Commit after every task. Repo root: `/home/sedhain_pankaj/Desktop/rust-hps-inventory`, branch `main`.
- The kiosk UI cannot be automated end-to-end here; UI tasks end with the syntax check + the manual smoke note listed in the task.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `ui/styles.css` | Modify | Timer/logo/date sizes, scan-status border removal, inline-table + week/day box styles |
| `ui/assets/noun-fingerprint-1377758.svg` | Modify | viewBox fix (glyph centered in ring) |
| `ui/js/app.js` | Modify | Partial re-render, menu/tabs/icons, Stocks/Moulds/Rates/Logs/Database/Staff-Cornice/Clock panels, `pendingCell`/`fmtBytes` helpers |
| `ui/js/auth.js` | Modify | Add `confirmModal`, remove `chooseClockAction` |
| `ui/js/icons.js` | Modify | Add `package`, `calculator`, `map-pin`, `save` icons |
| `ui/js/table.js` | **Create** | `createTableStore` + `mountInlineTable` (inline editing, drafts, bulk save) |
| `src-tauri/src/db.rs` | Modify | `run_data_migrations` + unit tests |
| `src-tauri/src/models.rs` | Modify | New/changed structs |
| `src-tauri/src/commands.rs` | Modify | New commands, `reserved` in stock, cornice-log amendment fields, remove cornice_stock commands (Task 10) |
| `src-tauri/src/lib.rs` | Modify | Command registration |

---

### Task 1: Visual fixes — timer, fingerprint SVG, scan-status bar

**Files:**
- Modify: `ui/styles.css:249-277` (clock logo/time/date), `ui/styles.css:877-911` (scan-status)
- Modify: `ui/assets/noun-fingerprint-1377758.svg` (line 1, viewBox)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing (pure visual). The `<10s` red pulse on the session pill already exists (`app.js:155-160`, `styles.css:384-389`) — verify only, no code change.

- [ ] **Step 1: Fix the fingerprint SVG viewBox**

In `ui/assets/noun-fingerprint-1377758.svg`, line 1, change:

```
viewBox="0 0 100 125" style="enable-background:new 0 0 100 100;"
```

to:

```
viewBox="0 0 100 100" style="enable-background:new 0 0 100 100;"
```

(The drawing occupies `0 0 100 100`; the 125-tall viewBox left dead space below the glyph, rendering it above the ring's center.)

- [ ] **Step 2: Timer — uniform seconds color, bigger logo, bigger date**

In `ui/styles.css`:

a) Delete this block entirely (lines 266-268):

```css
.clock-time .secs {
  color: var(--brass);
}
```

b) In `.clock-logo` (lines 249-255), change `width: 64px;` to `width: 100px;`.

c) In `.clock-date` (lines 270-277), change:

```css
  font-size: clamp(1.4rem, 3vw, 2.6rem);
```

to:

```css
  font-size: clamp(1.7rem, 3.5vw, 3.2rem);
```

- [ ] **Step 3: Remove the bracket-style left border from scan-status**

In `ui/styles.css`, in `.scan-status` (lines 877-888) delete the line `border-left-width: 4px;`. Then delete the `border-left-color` line from each variant:

- `.scan-status.info` (line 891): delete `border-left-color: var(--brass);`
- `.scan-status.warn` (line 896): delete `border-left-color: var(--warn);`
- `.scan-status.ok` (line 901): delete `border-left-color: var(--ok);`
- `.scan-status.err` (line 907): delete `border-left-color: var(--err);`

The variants keep their tinted `background` + `color`, which remains distinguishable.

- [ ] **Step 4: Verify**

Run: `node --check --input-type=module < ui/js/app.js` (unchanged file, sanity) — expected: no output (pass).
Manual smoke (kiosk): home clock shows uniform white/beige digits, logo ~100px, larger date; auth modal shows the fingerprint glyph centered in the pulsing rings; scan status bar has no left bracket bar.

- [ ] **Step 5: Commit**

```bash
git add ui/styles.css ui/assets/noun-fingerprint-1377758.svg
git commit -m "fix: uniform timer digits, larger logo/date, centered fingerprint glyph, no bracket on scan status"
```

---

### Task 2: Partial re-render on tab click (admin + staff)

**Files:**
- Modify: `ui/js/app.js:273-302` (`renderAdmin`), `ui/js/app.js:1252-1293` (`renderStaffDashboard`)

**Interfaces:**
- Consumes: `workspaceShell` (app.js:1750), `setPanel` (app.js:1795), `renderAdminPanel` (app.js:304), `renderStaffPanel` (app.js:1295).
- Produces: `switchAdminTab(id)` and `switchStaffTab(id)` — toggle `.active` on `[data-tab]` buttons and re-run only the panel renderer. The workspace shell (topbar + side nav) is built once per session.

- [ ] **Step 1: Replace `renderAdmin` tab-click handler**

In `ui/js/app.js`, replace the body of `renderAdmin` (lines 273-302) so the tab buttons no longer call `renderAdmin()`:

```js
function renderAdmin() {
  const tabs = [
    ["alerts", "Alerts"],
    ["employees", "Employees"],
    ["enroll", "Enroll"],
    ["payroll", "Payroll"],
    ["dispatch", "Dispatch"],
    ["cornice_stock", "Cornice Stock"],
    ["mould_inventory", "Moulds"],
    ["stock", "Stock"],
    ["rates", "Cornice Rates"],
    ["time", "Time Clock"],
    ["logs", "Daily Logs"],
    ["database", "Database"],
  ];
  app.innerHTML = workspaceShell(
    "Admin",
    state.admin?.name || "Admin",
    tabs,
    state.adminView,
  );
  app.querySelector("[data-back]").addEventListener("click", () => { endSession(); renderHome(); });
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      switchAdminTab(button.dataset.tab);
    });
  });
  renderAdminPanel();
}

function switchAdminTab(id) {
  if (id === state.adminView) return;
  state.adminView = id;
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.tab === id);
  });
  renderAdminPanel();
}
```

(The tab list here is unchanged in this task; Task 8 rewrites the list.)

- [ ] **Step 2: Replace `renderStaffDashboard` tab-click handler**

In `ui/js/app.js`, in `renderStaffDashboard` (lines 1252-1293) replace only the tab-click binding block:

```js
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.staffView = button.dataset.tab;
      renderStaffDashboard();
    });
  });
  renderStaffPanel();
```

with:

```js
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      switchStaffTab(button.dataset.tab);
    });
  });
  renderStaffPanel();
}

function switchStaffTab(id) {
  if (id === state.staffView) return;
  state.staffView = id;
  app.querySelectorAll("[data-tab]").forEach((button) => {
    button.classList.toggle("active", button.dataset.tab === id);
  });
  renderStaffPanel();
```

(Keep the existing closing brace structure correct — `switchStaffTab` is a new top-level function after `renderStaffDashboard`.)

- [ ] **Step 3: Verify**

Run: `node --check --input-type=module < ui/js/app.js` — expected: no output.
Manual smoke: open admin, click between tabs — DevTools shows the `.side-nav` DOM nodes persist (only `.panel-body`/`.panel-header` change); same for staff.

- [ ] **Step 4: Commit**

```bash
git add ui/js/app.js
git commit -m "fix: tab clicks re-render only the right pane, side nav persists"
```

---

### Task 3: DB migrations — reserved, amendment tracking, mould_locations, stock copy

**Files:**
- Modify: `src-tauri/src/db.rs` (add `run_data_migrations` + call it in `AppState::initialize` at db.rs:120-123; add `#[cfg(test)] mod tests` at end of file)

**Interfaces:**
- Consumes: existing `migrate(db)`, `run_column_migrations(db)`, `now_string()`, `_schema_migration_log` table.
- Produces:
  - `stock_items.reserved INTEGER NOT NULL DEFAULT 0`
  - `cornice_logs.prev_values TEXT` (nullable JSON), `cornice_logs.amended_at TEXT`, `cornice_logs.amended_by TEXT`
  - table `mould_locations (id INTEGER PK, name TEXT UNIQUE NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0)` seeded with `Near Dryer` (0), `Singles Wall` (1), `Doubles Wall` (2)
  - `cornice_stock` rows copied into `stock_items` as `item_type='cornice'` (aisle→location, quantity_in_stock→stock, quantity_reserved→reserved, remarks→notes)

- [ ] **Step 1: Write the failing tests**

Append to the end of `src-tauri/src/db.rs`:

```rust
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

        run_data_migrations(&pool).await;
        run_data_migrations(&pool).await;

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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test 2>&1 | tail -20`
Expected: compile FAIL — `run_data_migrations` not found.

- [ ] **Step 3: Implement `run_data_migrations`**

In `src-tauri/src/db.rs`, add after `run_column_migrations` (after line 579):

```rust
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
```

- [ ] **Step 4: Call the new migrations from `initialize`**

In `AppState::initialize` (db.rs:120-123), change:

```rust
        migrate(&db).await?;
        run_column_migrations(&db).await?;
        seed_assets(&db).await?;
```

to:

```rust
        migrate(&db).await?;
        run_column_migrations(&db).await?;
        run_data_migrations(&db).await?;
        seed_assets(&db).await?;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test 2>&1 | tail -20`
Expected: `test result: ok` with both new tests passing.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "feat: migrations for stock reserved, cornice log amendments, mould locations, stock copy"
```

---

### Task 4: Backend — `storage_status` command + disk alert

**Files:**
- Modify: `src-tauri/src/models.rs` (add `StorageStatus`)
- Modify: `src-tauri/src/commands.rs` (add `disk_usage` helper + `storage_status` command + test)
- Modify: `src-tauri/src/lib.rs` (register `storage_status`)

**Interfaces:**
- Consumes: `state.paths.db_path`, `db::notification`, `_schema` `admin_notifications`.
- Produces: command `storage_status` → `StorageStatus { db_path: String, db_size_bytes: u64, disk_total_bytes: u64, disk_free_bytes: u64, disk_used_pct: f64 }`. When `disk_used_pct > 90.0`, inserts a red `disk_space` alert unless an unresolved `disk_space` alert already exists.

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/commands.rs` (end of file):

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test storage_tests 2>&1 | tail -10`
Expected: compile FAIL — `disk_usage` not found.

- [ ] **Step 3: Add the model**

In `src-tauri/src/models.rs`, append:

```rust
// Storage / disk status
#[derive(Debug, Clone, Serialize)]
pub struct StorageStatus {
    pub db_path: String,
    pub db_size_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_free_bytes: u64,
    pub disk_used_pct: f64,
}
```

- [ ] **Step 4: Add the helper + command**

In `src-tauri/src/commands.rs`, add near the other helpers (e.g. after `parse_timestamp`, before `to_string`):

```rust
fn disk_usage(path: &std::path::Path) -> Option<(u64, u64, f64)> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
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
    format!("{} {}", if value >= 100.0 || index == 0 { value as u64 } else { (value * 10.0) as u64 / 10 }, UNITS[index])
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
                        &format!("Disk is {pct:.0}% full — only {} free. Free up space.", human_bytes(free)),
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
```

- [ ] **Step 5: Register the command**

In `src-tauri/src/lib.rs`, in the `generate_handler!` list, add `storage_status,` after `app_status,`.

- [ ] **Step 6: Verify**

Run: `cd src-tauri && cargo check && cargo test storage_tests 2>&1 | tail -10`
Expected: check passes; test passes.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: storage_status command with disk usage and >90% disk_space alert"
```

---

### Task 5: Backend — stock `reserved`, delete commands, mould locations CRUD

**Files:**
- Modify: `src-tauri/src/models.rs` (`StockItem`, `StockItemInput`, add `MouldLocation`, `MouldLocationInput`)
- Modify: `src-tauri/src/commands.rs` (stock list/save + `delete_stock_item`, `delete_could... ` — see steps; mould section)
- Modify: `src-tauri/src/lib.rs` (register 5 new commands)

**Interfaces:**
- Consumes: Task 3 migration (`stock_items.reserved`, `mould_locations`).
- Produces:
  - `StockItem.reserved: i64`, `StockItemInput.reserved: i64` (serde default)
  - `delete_stock_item(id: i64)`, `delete_cornice_rate(id: i64)`, `delete_mould_inventory(id: i64)` → `CommandResult<()>`
  - `MouldLocation { id: i64, name: String, sort_order: i64 }`, `MouldLocationInput { id: Option<i64>, name: String }`
  - `list_mould_locations() -> Vec<MouldLocation>` (ordered by `sort_order, id`)
  - `save_mould_location(input) -> MouldLocation` (insert with next `sort_order`, or rename by id)
  - `delete_mould_location(id: i64) -> ()` (errors if any `mould_inventory.storage_location` references it)

- [ ] **Step 1: Update stock models**

In `src-tauri/src/models.rs`, in `StockItem` add `pub reserved: i64,` after `pub stock: i64,`. In `StockItemInput` add:

```rust
    #[serde(default)]
    pub reserved: i64,
```

after `pub stock: i64,`.

- [ ] **Step 2: Add mould location models**

Append to `src-tauri/src/models.rs`:

```rust
// Mould Locations
#[derive(Debug, Clone, Serialize)]
pub struct MouldLocation {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MouldLocationInput {
    pub id: Option<i64>,
    pub name: String,
}
```

- [ ] **Step 3: Update stock commands + add delete commands**

In `src-tauri/src/commands.rs`:

a) `list_stock_items` (line ~881): change the SELECT to:

```rust
        SELECT id, item_type, model, stock, reserved, location, dimensions, photo_path, notes
        FROM stock_items
        ORDER BY item_type, model COLLATE NOCASE
```

and add `reserved: row.get("reserved"),` after `stock: row.get("stock"),` in the mapping.

b) `save_stock_item` (line ~908): in the UPDATE add `reserved = ?` (bind `input.reserved` after `input.stock`) and in the INSERT add `reserved` to the column list and `?` to the values (bind `input.reserved` after `input.stock`).

c) `stock_item_by_id` (line ~2803): add `reserved` to the SELECT and `reserved: row.get("reserved"),` to the mapping.

d) Append after `save_cornice_rate` (line ~1029):

```rust
#[tauri::command]
pub async fn delete_stock_item(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("DELETE FROM stock_items WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

#[tauri::command]
pub async fn delete_cornice_rate(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("DELETE FROM cornice_rates WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}
```

- [ ] **Step 4: Add mould location commands**

In `src-tauri/src/commands.rs`, in the `// ==================== Mould Inventory ====================` section, append after `save_mould_inventory` (line ~1677):

```rust
#[tauri::command]
pub async fn delete_mould_inventory(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("DELETE FROM mould_inventory WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

// ==================== Mould Locations ====================

#[tauri::command]
pub async fn list_mould_locations(state: State<'_, AppState>) -> CommandResult<Vec<MouldLocation>> {
    let rows = sqlx::query("SELECT id, name, sort_order FROM mould_locations ORDER BY sort_order, id")
        .fetch_all(&state.db)
        .await
        .map_err(to_string)?;
    Ok(rows.into_iter().map(mould_location_from_row).collect())
}

#[tauri::command]
pub async fn save_mould_location(
    state: State<'_, AppState>,
    input: MouldLocationInput,
) -> CommandResult<MouldLocation> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("Location name is required.".to_string());
    }
    if let Some(id) = input.id {
        sqlx::query("UPDATE mould_locations SET name = ? WHERE id = ?")
            .bind(&name)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        let row = sqlx::query("SELECT id, name, sort_order FROM mould_locations WHERE id = ?")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(to_string)?;
        Ok(mould_location_from_row(row))
    } else {
        let next: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM mould_locations")
            .fetch_one(&state.db)
            .await
            .map_err(to_string)?;
        sqlx::query("INSERT INTO mould_locations (name, sort_order) VALUES (?, ?)")
            .bind(&name)
            .bind(next)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        let row = sqlx::query("SELECT id, name, sort_order FROM mould_locations WHERE name = ? ORDER BY id DESC LIMIT 1")
            .bind(&name)
            .fetch_one(&state.db)
            .await
            .map_err(to_string)?;
        Ok(mould_location_from_row(row))
    }
}

#[tauri::command]
pub async fn delete_mould_location(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM mould_locations WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_string)?;
    let name = name.ok_or_else(|| "Location not found.".to_string())?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mould_inventory WHERE storage_location = ?")
        .bind(&name)
        .fetch_one(&state.db)
        .await
        .map_err(to_string)?;
    if count > 0 {
        return Err(format!("Cannot delete: {count} mould(s) are stored in this location."));
    }
    sqlx::query("DELETE FROM mould_locations WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

fn mould_location_from_row(row: sqlx::sqlite::SqliteRow) -> MouldLocation {
    MouldLocation {
        id: row.get("id"),
        name: row.get("name"),
        sort_order: row.get("sort_order"),
    }
}
```

- [ ] **Step 5: Register commands**

In `src-tauri/src/lib.rs` `generate_handler!`, add:

```rust
            delete_stock_item,
            delete_cornice_rate,
            delete_mould_inventory,
            list_mould_locations,
            save_mould_location,
            delete_mould_location,
```

(after `save_mould_inventory,`).

- [ ] **Step 6: Verify**

Run: `cd src-tauri && cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: stock reserved field, delete commands, mould locations CRUD"
```

---

### Task 6: Backend — cornice log update/delete/approve (amendment workflow)

**Files:**
- Modify: `src-tauri/src/models.rs` (`CorniceLog` +3 fields, add `CorniceLogUpdateInput`)
- Modify: `src-tauri/src/commands.rs` (`cornice_log_from_row`, `CORNICE_LOG_COLUMNS`, 3 new commands)
- Modify: `src-tauri/src/lib.rs` (register 3 commands)

**Interfaces:**
- Consumes: Task 3 columns (`prev_values`, `amended_at`, `amended_by`), `find_rate_for_model` (commands.rs:2836), `db::notification`, `today_string()`.
- Produces:
  - `CorniceLog` gains `prev_values: Option<String>`, `amended_at: Option<String>`, `amended_by: Option<String>`
  - `CorniceLogUpdateInput { id: i64, actor_id: String, series: String, model: String, lengths: i64 }`
  - `update_cornice_log(input) -> CorniceLog`: actor must own the entry. Same-day: applies immediately, rate re-looked-up, `needs_admin_review = is_custom || unit_value.is_none()`, sets `amended_at/by`. Older-day: stores previous changed values in `prev_values` (JSON object, e.g. `{"model":"X","lengths":8}`), forces `needs_admin_review = 1`, yellow `cornice_log_edit` alert.
  - `delete_cornice_log(id, actor_id) -> ()`: same-day → hard delete. Older-day → staged delete: `prev_values = {"deleted":true,"model":...,"lengths":...,"total_units":...}`, `needs_admin_review = 1`, yellow alert.
  - `approve_cornice_log(id) -> ()`: staged delete → hard delete; otherwise clears `prev_values` + `needs_admin_review`; resolves linked `cornice_log_edit` alerts.

- [ ] **Step 1: Update models**

In `src-tauri/src/models.rs`, in `CorniceLog` add after `pub needs_admin_review: bool,`:

```rust
    pub prev_values: Option<String>,
    pub amended_at: Option<String>,
    pub amended_by: Option<String>,
```

Append after `CorniceLog`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct CorniceLogUpdateInput {
    pub id: i64,
    pub actor_id: String,
    pub series: String,
    pub model: String,
    pub lengths: i64,
}
```

- [ ] **Step 2: Update `cornice_log_from_row`**

In `src-tauri/src/commands.rs` (line ~3094), in the `Ok(CorniceLog { ... })` literal add after `needs_admin_review: ...`:

```rust
        prev_values: row.try_get::<Option<String>, _>("prev_values").unwrap_or(None),
        amended_at: row.try_get::<Option<String>, _>("amended_at").unwrap_or(None),
        amended_by: row.try_get::<Option<String>, _>("amended_by").unwrap_or(None),
```

- [ ] **Step 3: Expose new columns in the admin table editor**

In `CORNICE_LOG_COLUMNS` (commands.rs:97-111), add before `col("created_at", ...)`:

```rust
    readonly_col("prev_values", "Previous Values", AdminColumnKind::Text),
    readonly_col("amended_at", "Amended At", AdminColumnKind::Text),
    readonly_col("amended_by", "Amended By", AdminColumnKind::Text),
```

- [ ] **Step 4: Add the three commands**

In `src-tauri/src/commands.rs`, append after `list_cornice_logs` (line ~1470):

```rust
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
    let (series, unit_text, unit_value, is_custom) = match rate {
        Some(rate) => (rate.series, rate.unit_text, rate.unit_value, false),
        None => (input.series.trim().to_string(), String::new(), None, true),
    };
    let total_units = unit_value.unwrap_or(0.0) * input.lengths as f64;
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
        SET series = ?, model = ?, lengths = ?, unit_text = ?, unit_value = ?, total_units = ?,
            is_custom = ?, needs_admin_review = ?, prev_values = ?, amended_at = ?, amended_by = ?
        WHERE id = ?
        "#,
    )
    .bind(series)
    .bind(input.model.trim())
    .bind(input.lengths)
    .bind(unit_text)
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
```

- [ ] **Step 5: Register commands**

In `src-tauri/src/lib.rs` `generate_handler!`, add after `list_cornice_logs,`:

```rust
            update_cornice_log,
            delete_cornice_log,
            approve_cornice_log,
```

- [ ] **Step 6: Verify**

Run: `cd src-tauri && cargo check && cargo test 2>&1 | tail -10`
Expected: check passes; all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: cornice log update/delete/approve with same-day vs pending approval rules"
```

---

### Task 7: Backend — `get_clock_status`

**Files:**
- Modify: `src-tauri/src/models.rs` (add `ClockStatus`)
- Modify: `src-tauri/src/commands.rs` (add command)
- Modify: `src-tauri/src/lib.rs` (register)

**Interfaces:**
- Consumes: `time_clock_events`, `today_string()`.
- Produces: command `get_clock_status(employee_id: String) -> ClockStatus { today_state: String /* "none" | "in" | "out" */, missed_yesterday_clock_out: bool }`. `missed_yesterday_clock_out` = yesterday has a `clock_in` with no later same-day `clock_out` (read-only; does not write alerts — `record_clock_event` already raises the red `missing_clock_out` alert via `refresh_attendance_issues`).

- [ ] **Step 1: Add the model**

Append to `src-tauri/src/models.rs`:

```rust
// Clock status for the staff clock button
#[derive(Debug, Clone, Serialize)]
pub struct ClockStatus {
    pub today_state: String,
    pub missed_yesterday_clock_out: bool,
}
```

- [ ] **Step 2: Add the command**

In `src-tauri/src/commands.rs`, append after `list_clock_events` (line ~1154):

```rust
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
```

- [ ] **Step 3: Register the command**

In `src-tauri/src/lib.rs` `generate_handler!`, add `get_clock_status,` after `list_clock_events,`.

- [ ] **Step 4: Verify**

Run: `cd src-tauri && cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: get_clock_status for smart clock in/out button"
```

---

### Task 8: UI — new icons, menu labels, tabIcons

**Files:**
- Modify: `ui/js/icons.js` (4 new icons)
- Modify: `ui/js/app.js` (`tabIcons` at 1728-1748, admin tab list at 274-287, staff tab list at 1255-1282)

**Interfaces:**
- Consumes: `icon(name, size)` renderer (icons.js:24).
- Produces: icon names `package`, `calculator`, `map-pin`, `save` (used by Tasks 9-16). Admin menu: `Cornice Stock` tab removed, `Moulds` → `Mould Locations`, `Stock` → `Stocks`. Staff: storekeeper `Stock` tab (id `cornice_stock`) now routes to the merged stock panel (Task 10), helper `Stock` tab (id `cornice_stock_ro`) routes to read-only merged stock (Task 10).

- [ ] **Step 1: Add icons**

In `ui/js/icons.js`, inside the `ICONS` object, add after the `refresh` entry (line 21):

```js
  package: '<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.29 7 12 12 20.71 7"/><line x1="12" x2="12" y1="22" y2="12"/>',
  calculator: '<rect width="16" height="20" x="4" y="2" rx="2"/><line x1="8" x2="16" y1="6" y2="6"/><line x1="16" x2="16" y1="14" y2="18"/><path d="M16 10h.01"/><path d="M12 10h.01"/><path d="M8 10h.01"/><path d="M12 14h.01"/><path d="M8 14h.01"/><path d="M12 18h.01"/><path d="M8 18h.01"/>',
  "map-pin": '<path d="M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z"/><circle cx="12" cy="10" r="3"/>',
  save: '<path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/>',
```

- [ ] **Step 2: Update `tabIcons`**

In `ui/js/app.js`, replace the whole `tabIcons` object (lines 1728-1748) with:

```js
const tabIcons = {
  alerts: "bell",
  employees: "users",
  enroll: "fingerprint",
  payroll: "dollar",
  dispatch: "truck",
  mould_inventory: "map-pin",
  stock: "package",
  rates: "calculator",
  time: "clock",
  logs: "list",
  database: "database",
  clock: "clock",
  cornice: "box",
  moulds: "map-pin",
  production: "gauge",
  deliveries: "truck",
  cornice_stock: "package",
  cornice_stock_ro: "package",
  overstock: "box",
};
```

- [ ] **Step 3: Update the admin tab list**

In `renderAdmin` (app.js ~274-287), replace the tabs array with:

```js
  const tabs = [
    ["alerts", "Alerts"],
    ["employees", "Employees"],
    ["enroll", "Enroll"],
    ["payroll", "Payroll"],
    ["dispatch", "Dispatch"],
    ["mould_inventory", "Mould Locations"],
    ["stock", "Stocks"],
    ["rates", "Cornice Rates"],
    ["time", "Time Clock"],
    ["logs", "Daily Logs"],
    ["database", "Database"],
  ];
```

(Leaving the `cornice_stock` dispatch in `renderAdminPanel` for now — Task 10 removes it.)

- [ ] **Step 4: Verify**

Run: `node --check --input-type=module < ui/js/app.js && node --check --input-type=module < ui/js/icons.js`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add ui/js/icons.js ui/js/app.js
git commit -m "feat: new icons for stocks/rates/mould locations, updated menu labels"
```

---

### Task 9: UI — inline table component (`ui/js/table.js`) + CSS

**Files:**
- Create: `ui/js/table.js`
- Modify: `ui/styles.css` (append inline-table styles at end of file, before the `@media (prefers-reduced-motion)` block is fine — append at the very end instead)

**Interfaces:**
- Consumes: `escapeHtml` from `./api.js`, `icon` from `./icons.js` (icons `plus`, `x`, `save`, `refresh` — `save` from Task 8).
- Produces (used by Tasks 10-15):
  - `createTableStore({ commit: { add(values), save(values), remove(id) }, onDone })` → store with: `drafts` (Map), `draftFor(row)`, `addNew(tableId, defaults)`, `setValues(row, partialValues)`, `stageDelete(row)`, `unstageDelete(row)`, `hasPending()`, `discardAll()`, `saveAll()`, `renderActions(actionsEl, { refreshFn, extraActions, onRendered })`.
  - `mountInlineTable(rootEl, store, { columns, rows, tableId, emptyText, canEdit, canDelete, cellHtml, rowClass, actionsEl, refreshFn, extraActions, onActionsRendered })` → `{ render() }`.
  - Column shape: `{ key, label, type: "text"|"number"|"select"|"bool", options?: string[], editable?: boolean, align?: "right", list?: string (datalist id) }`.
  - Draft row keys: existing rows keyed by `id-<id>`, new rows by `__key` (`new-N`).

- [ ] **Step 1: Create `ui/js/table.js`**

```js
import { escapeHtml } from "./api.js";
import { icon } from "./icons.js";

// Draft store shared by one or more inline tables within a single panel.
export function createTableStore({ commit, onDone }) {
  const drafts = new Map();
  let seq = 0;

  function draftFor(row) {
    const key = row.id != null ? `id-${row.id}` : row.__key;
    if (key == null) return null;
    if (!drafts.has(key)) {
      drafts.set(key, {
        row: { ...row },
        values: null,
        dirty: false,
        deleted: false,
        isNew: false,
        tableId: null,
      });
    }
    return drafts.get(key);
  }

  function addNew(tableId, defaults) {
    const key = `new-${++seq}`;
    const row = { ...defaults, id: null, __key: key };
    drafts.set(key, {
      row,
      values: { ...defaults },
      dirty: true,
      deleted: false,
      isNew: true,
      tableId,
    });
    return row;
  }

  function setValues(row, values) {
    const draft = draftFor(row);
    if (!draft) return;
    draft.values = { ...(draft.values || {}), ...values };
    if (draft.isNew) draft.row = { ...draft.row, ...values };
    draft.dirty = true;
  }

  function stageDelete(row) {
    const draft = draftFor(row);
    if (draft) {
      draft.deleted = true;
      draft.dirty = false;
    }
  }

  function unstageDelete(row) {
    const draft = draftFor(row);
    if (draft) draft.deleted = false;
  }

  function pendingDrafts() {
    return [...drafts.values()].filter((draft) => draft.deleted || (draft.dirty && draft.values));
  }

  function hasPending() {
    return pendingDrafts().length > 0;
  }

  function discardAll() {
    drafts.clear();
  }

  async function saveAll() {
    const pending = pendingDrafts();
    if (!pending.length) return;
    for (const draft of pending) {
      if (draft.deleted) {
        if (draft.row.id != null) await commit.remove(draft.row.id);
      } else if (draft.isNew) {
        await commit.add(draft.values);
      } else {
        await commit.save({ ...draft.row, ...draft.values });
      }
    }
    drafts.clear();
    if (onDone) await onDone();
  }

  function renderActions(actionsEl, { refreshFn, extraActions = "", onRendered = null }) {
    if (!actionsEl) return;
    const right = hasPending()
      ? `<button class="icon primary" data-store-save title="Save changes">${icon("save", 20)}</button>
         <button class="icon ghost" data-store-discard title="Discard changes">${icon("x", 20)}</button>`
      : `<button class="ghost" data-store-refresh>${icon("refresh", 18)} Refresh</button>`;
    actionsEl.innerHTML = `${extraActions}${right}`;
    actionsEl.querySelector("[data-store-save]")?.addEventListener("click", () => {
      saveAll().catch((error) => {
        alert(String((error && error.message) || error));
      });
    });
    actionsEl.querySelector("[data-store-discard]")?.addEventListener("click", () => {
      discardAll();
      if (refreshFn) refreshFn();
    });
    actionsEl.querySelector("[data-store-refresh]")?.addEventListener("click", () => {
      if (refreshFn) refreshFn();
    });
    if (onRendered) onRendered(actionsEl);
  }

  return {
    drafts,
    draftFor,
    addNew,
    setValues,
    stageDelete,
    unstageDelete,
    hasPending,
    discardAll,
    saveAll,
    renderActions,
  };
}

// Mount one inline-editable table into rootEl, backed by a shared store.
export function mountInlineTable(rootEl, store, config) {
  const {
    columns,
    rows,
    tableId = "main",
    emptyText = "No records",
    canEdit = () => true,
    canDelete = () => true,
    cellHtml = null,
    rowClass = () => "",
    actionsEl = null,
    refreshFn = null,
    extraActions = "",
    onActionsRendered = null,
  } = config;

  function valueOf(row, col) {
    const draft = store.draftFor(row);
    if (draft && draft.values && draft.values[col.key] !== undefined) return draft.values[col.key];
    return row[col.key];
  }

  function displayCell(row, col) {
    if (cellHtml) {
      const custom = cellHtml(row, col);
      if (custom != null) return custom;
    }
    const value = valueOf(row, col);
    if (value === null || value === undefined || value === "") return "";
    if (col.type === "bool") return value ? "Yes" : "No";
    return escapeHtml(String(value));
  }

  function editControl(row, col) {
    const value = valueOf(row, col);
    if (col.type === "select") {
      const options = (col.options || [])
        .map(
          (opt) =>
            `<option value="${escapeHtml(opt)}" ${String(value) === String(opt) ? "selected" : ""}>${escapeHtml(opt)}</option>`,
        )
        .join("");
      return `<select data-edit data-key="${col.key}">${options}</select>`;
    }
    if (col.type === "bool") {
      return `<select data-edit data-key="${col.key}">
        <option value="true" ${value ? "selected" : ""}>Yes</option>
        <option value="false" ${value ? "" : "selected"}>No</option>
      </select>`;
    }
    const type = col.type === "number" ? "number" : "text";
    const listAttr = col.list ? ` list="${col.list}"` : "";
    return `<input data-edit data-key="${col.key}" type="${type}" value="${escapeHtml(value === null || value === undefined ? "" : value)}"${listAttr} ${col.type === "number" ? 'step="any" min="0"' : ""} />`;
  }

  function allRows() {
    const out = [...rows];
    for (const draft of store.drafts.values()) {
      if (draft.isNew && draft.tableId === tableId) out.push(draft.row);
    }
    return out;
  }

  function rowKeyOf(row) {
    return row.id != null ? String(row.id) : row.__key;
  }

  function findRow(tr) {
    const key = tr.dataset.rowkey;
    return allRows().find((row) => rowKeyOf(row) === key) || null;
  }

  function refreshActions() {
    if (actionsEl) {
      store.renderActions(actionsEl, { refreshFn, extraActions, onRendered: onActionsRendered });
    }
  }

  function render() {
    const data = allRows();
    if (!data.length) {
      rootEl.innerHTML = `<div class="message">${escapeHtml(emptyText)}</div>`;
      refreshActions();
      return;
    }
    rootEl.innerHTML = `
      <div class="table-wrap">
        <table class="table itable">
          <thead>
            <tr>
              ${columns.map((col) => `<th class="${col.align === "right" ? "num" : ""}">${escapeHtml(col.label)}</th>`).join("")}
              <th class="row-actions-head"></th>
            </tr>
          </thead>
          <tbody>
            ${data
              .map((row) => {
                const draft = store.draftFor(row);
                const cls = [
                  draft && draft.deleted ? "staged-delete" : "",
                  draft && draft.dirty ? "dirty" : "",
                  rowClass(row),
                ]
                  .filter(Boolean)
                  .join(" ");
                return `
                  <tr class="${cls}" data-rowkey="${escapeHtml(rowKeyOf(row))}">
                    ${columns
                      .map(
                        (col) => `
                        <td data-cell data-key="${col.key}" class="${col.editable && canEdit(row) ? "editable" : ""} ${col.align === "right" ? "num" : ""}">
                          ${displayCell(row, col)}
                        </td>`,
                      )
                      .join("")}
                    <td class="row-actions">
                      ${canDelete(row) ? `<button class="icon ghost" data-del title="Delete">${icon("x", 16)}</button>` : ""}
                    </td>
                  </tr>`;
              })
              .join("")}
          </tbody>
        </table>
      </div>
    `;
    bind();
    refreshActions();
  }

  function startEdit(td) {
    if (td.querySelector("[data-edit]")) return;
    const row = findRow(td.closest("tr"));
    const col = columns.find((c) => c.key === td.dataset.key);
    if (!row || !col) return;
    td.innerHTML = editControl(row, col);
    const input = td.querySelector("[data-edit]");
    input.focus();
    if (typeof input.select === "function") input.select();

    let committed = false;
    const commitCell = () => {
      if (committed) return;
      committed = true;
      let value = input.value;
      if (col.type === "number") value = value === "" ? 0 : Number(value);
      if (col.type === "bool") value = value === "true";
      store.setValues(row, { [col.key]: value });
      render();
    };
    input.addEventListener("change", commitCell);
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        commitCell();
      } else if (event.key === "Escape") {
        event.preventDefault();
        committed = true;
        render();
      }
    });
    input.addEventListener("blur", () => {
      if (!committed) {
        committed = true;
        render();
      }
    });
  }

  function bind() {
    rootEl.querySelectorAll("td.editable").forEach((td) => {
      td.addEventListener("click", () => startEdit(td));
    });
    rootEl.querySelectorAll("[data-del]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const row = findRow(btn.closest("tr"));
        if (!row) return;
        const draft = store.draftFor(row);
        if (draft && draft.deleted) store.unstageDelete(row);
        else store.stageDelete(row);
        render();
      });
    });
  }

  render();
  return { render };
}
```

- [ ] **Step 2: Append CSS**

Append to the end of `ui/styles.css`:

```css
/* ---------- Inline editable tables ---------- */

.table td.editable {
  cursor: pointer;
}

.table td.editable:hover {
  background: var(--bg-2);
  box-shadow: inset 0 0 0 1px var(--line-2);
}

.itable td input,
.itable td select {
  padding: 8px 10px;
  min-height: 0;
}

.table tr.dirty td {
  background: var(--brass-tint);
}

.table tr.staged-delete td {
  opacity: 0.45;
  text-decoration: line-through;
}

.table td.num,
.table th.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.table .row-actions {
  width: 48px;
}

.table .row-actions button {
  min-height: 36px;
  padding: 0 8px;
}

.table .row-actions button svg {
  width: 16px;
  height: 16px;
}

.old-new s {
  color: var(--text-3);
}

.amended {
  color: var(--brass);
  cursor: help;
  margin-left: 4px;
}

/* ---------- Week / day boxes (staff log, mould locations) ---------- */

.week-box {
  border: 1px solid var(--line-2);
  border-radius: var(--radius-l);
  padding: 16px;
  margin-bottom: 18px;
  background: var(--bg-0);
}

.day-box {
  border: 1px solid var(--line);
  border-radius: var(--radius-m);
  padding: 14px;
  margin-bottom: 14px;
  background: var(--bg-1);
}

.week-box > h3,
.day-box > h3 {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin: 0 0 12px;
  font-size: 1rem;
  font-weight: 700;
  text-transform: none;
  letter-spacing: 0;
  color: var(--text-1);
}

.week-box > h3 small,
.day-box > h3 small {
  color: var(--text-3);
  font-weight: 600;
}

.day-box:last-child {
  margin-bottom: 0;
}
```

- [ ] **Step 3: Verify**

Run: `node --check --input-type=module < ui/js/table.js`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add ui/js/table.js ui/styles.css
git commit -m "feat: shared inline table component with drafts, bulk save, and week/day box styles"
```

---

### Task 10: UI — merged Stocks panel (admin + storekeeper + helper RO); remove cornice_stock

**Files:**
- Modify: `ui/js/app.js` (replace `renderStockPanel` at 598-662, delete `renderCorniceStockPanel` at 1060-1113, replace `renderStaffCorniceStockRO` at 1653-1668, update `renderAdminPanel` dispatch at 304-317, `renderStaffPanel` dispatch at 1295-1308, `state` at 14-35)
- Modify: `src-tauri/src/commands.rs` (delete `list_cornice_stock` + `save_cornice_stock` at 1679-1745 and their `*_by_id`/`*_from_row` helpers)
- Modify: `src-tauri/src/models.rs` (delete `CorniceStock`, `CorniceStockInput`)
- Modify: `src-tauri/src/lib.rs` (remove `list_cornice_stock, save_cornice_stock,`)

**Interfaces:**
- Consumes: `createTableStore`/`mountInlineTable` (Task 9), `list_stock_items`/`save_stock_item`/`delete_stock_item` with `reserved` (Task 5), `icon("package")` (Task 8).
- Produces: `renderStockPanel()` (merged, inline-editable, type filter All/Cornice/Other), `renderStaffStockRO()` (read-only merged stock). `state.stockFilter` (`"all" | "cornice" | "other"`).

- [ ] **Step 1: Add `stockFilter` to state**

In `ui/js/app.js` `state` (lines 14-35), replace `selectedStock: null,` and `selectedCorniceStock: null,` with a single line:

```js
  stockFilter: "all",
```

(delete both old lines; keep `selectedRate`/`selectedMould` until Tasks 11-12 remove them).

- [ ] **Step 2: Replace `renderStockPanel`**

Replace the entire `renderStockPanel` function (app.js:598-662) with:

```js
async function renderStockPanel() {
  const items = await invoke("list_stock_items");
  const filter = state.stockFilter || "all";
  const rows = items.filter((item) =>
    filter === "all"
      ? true
      : filter === "cornice"
        ? item.item_type === "cornice"
        : item.item_type !== "cornice",
  );
  const store = createTableStore({
    commit: {
      add: (values) =>
        invoke("save_stock_item", {
          input: {
            id: null,
            item_type: values.item_type || "cornice",
            model: values.model,
            stock: values.stock,
            reserved: values.reserved,
            location: values.location,
            dimensions: values.dimensions,
            photo_path: values.photo_path,
            notes: values.notes,
          },
        }),
      save: (values) =>
        invoke("save_stock_item", {
          input: {
            id: values.id,
            item_type: values.item_type,
            model: values.model,
            stock: values.stock,
            reserved: values.reserved,
            location: values.location,
            dimensions: values.dimensions,
            photo_path: values.photo_path,
            notes: values.notes,
          },
        }),
      remove: (id) => invoke("delete_stock_item", { id }),
    },
    onDone: renderStockPanel,
  });
  const actionsEl = () => app.querySelector("[data-panel-actions]");
  setPanel(
    "Stocks",
    `<select data-stock-filter style="width:160px">
      <option value="all" ${filter === "all" ? "selected" : ""}>All types</option>
      <option value="cornice" ${filter === "cornice" ? "selected" : ""}>Cornice</option>
      <option value="other" ${filter === "other" ? "selected" : ""}>Other</option>
    </select>`,
    `<div data-stock-table></div>`,
  );
  const mounted = mountInlineTable(app.querySelector("[data-stock-table]"), store, {
    columns: [
      { key: "item_type", label: "Type", type: "select", options: ["cornice", "other"], editable: true },
      { key: "model", label: "Model", type: "text", editable: true },
      { key: "location", label: "Location", type: "text", editable: true },
      { key: "stock", label: "Stock", type: "number", editable: true, align: "right" },
      { key: "reserved", label: "Reserved", type: "number", editable: true, align: "right" },
      { key: "notes", label: "Notes", type: "text", editable: true },
    ],
    rows,
    tableId: "main",
    emptyText: "No stock items",
    actionsEl: actionsEl(),
    refreshFn: renderStockPanel,
    extraActions: `<button class="ghost" data-add-stock>${icon("plus", 18)} Add</button>`,
    onActionsRendered: (el) => {
      el.querySelector("[data-add-stock]")?.addEventListener("click", () => {
        store.addNew("main", {
          item_type: "cornice",
          model: "",
          location: "",
          stock: 0,
          reserved: 0,
          notes: "",
          dimensions: "",
          photo_path: "",
        });
        mounted.render();
      });
    },
  });
  app.querySelector("[data-stock-filter]").addEventListener("change", (event) => {
    state.stockFilter = event.currentTarget.value;
    renderStockPanel();
  });
}
```

- [ ] **Step 3: Delete `renderCorniceStockPanel`, add `renderStaffStockRO`**

Delete the entire `renderCorniceStockPanel` function (app.js:1060-1113, including its `// ==================== Admin: Cornice Stock Panel ====================` comment). Replace `renderStaffCorniceStockRO` (app.js:1653-1668) with:

```js
async function renderStaffStockRO() {
  const items = await invoke("list_stock_items");
  setPanel(
    "Stocks (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    table(
      ["Type", "Model", "Location", "Stock", "Reserved"],
      items.map((item) => ({
        cells: [item.item_type, item.model, item.location, item.stock, item.reserved],
      })),
    ),
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffStockRO);
}
```

- [ ] **Step 4: Update dispatchers + import**

a) In `renderAdminPanel` (app.js:304-317) delete the line:

```js
  if (state.adminView === "cornice_stock") return renderCorniceStockPanel();
```

b) In `renderStaffPanel` (app.js:1295-1308) change:

```js
  if (view === "cornice_stock") return renderCorniceStockPanel();
  if (view === "cornice_stock_ro") return renderStaffCorniceStockRO();
```

to:

```js
  if (view === "cornice_stock") return renderStockPanel();
  if (view === "cornice_stock_ro") return renderStaffStockRO();
```

c) At the top of `ui/js/app.js` (line 10) add the import:

```js
import { createTableStore, mountInlineTable } from "./table.js";
```

- [ ] **Step 5: Remove cornice_stock backend**

a) In `src-tauri/src/commands.rs` delete `list_cornice_stock` and `save_cornice_stock` (lines 1679-1745 including the `// ==================== Cornice Stock ====================` header), plus their private helpers `cornice_stock_by_id` and `cornice_stock_from_row` (search for `fn cornice_stock_by_id` / `fn cornice_stock_from_row` near the end of the file and delete both).

b) In `src-tauri/src/models.rs` delete `CorniceStock` and `CorniceStockInput` (lines 307-327 including the comment).

c) In `src-tauri/src/lib.rs` delete the lines `list_cornice_stock,` and `save_cornice_stock,` (and the `// New: cornice stock` comment).

- [ ] **Step 6: Verify**

Run: `node --check --input-type=module < ui/js/app.js` and `cd src-tauri && cargo check 2>&1 | tail -5`
Expected: both pass.

- [ ] **Step 7: Manual smoke note**

Kiosk: admin → Stocks shows merged rows (type filter works, cornice rows migrated with reserved populated); click a cell to edit, dirty row highlights, save icon appears, save persists; Add appends an editable row; ✕ stages delete (row dims) and save removes it; storekeeper sees editable Stocks; helper sees read-only Stocks.

- [ ] **Step 8: Commit**

```bash
git add ui/js/app.js src-tauri/src/commands.rs src-tauri/src/models.rs src-tauri/src/lib.rs
git commit -m "feat: merge cornice stock into Stocks with inline editing; drop cornice_stock commands"
```

---

### Task 11: UI — Mould Locations panel (admin + staff RO)

**Files:**
- Modify: `ui/js/app.js` (replace `renderMouldInventoryPanel` at 1115-1164 and `renderStaffMouldView` at 1636-1651; remove `selectedMould` from state)

**Interfaces:**
- Consumes: `createTableStore`/`mountInlineTable` (Task 9), `list_mould_locations`/`save_mould_location`/`delete_mould_location` (Task 5), `list_mould_inventory`/`save_mould_inventory`/`delete_mould_inventory` (Task 5).
- Produces: `renderMouldLocationsPanel()` — one `.day-box` per location (header: name + count + per-box Add + delete-location), trailing Unassigned box for legacy rows, panel-level Add Location. `renderStaffMouldView()` — same boxes, read-only.

- [ ] **Step 1: Remove `selectedMould` from state**

In `ui/js/app.js` `state`, delete the line `selectedMould: null,`.

- [ ] **Step 2: Replace `renderMouldInventoryPanel`**

Replace the entire function (app.js:1115-1164, including its section comment header — rename the comment to `// ==================== Admin: Mould Locations Panel ====================`) with:

```js
async function renderMouldLocationsPanel() {
  const [locations, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_inventory"),
  ]);
  const store = createTableStore({
    commit: {
      add: (values) => invoke("save_mould_inventory", { input: values }),
      save: (values) => invoke("save_mould_inventory", { input: values }),
      remove: (id) => invoke("delete_mould_inventory", { id }),
    },
    onDone: renderMouldLocationsPanel,
  });
  const actionsEl = app.querySelector("[data-panel-actions]");
  const locationColumns = [
    { key: "mould_name", label: "Mould Name", type: "text", editable: true },
    {
      key: "storage_location",
      label: "Location",
      type: "select",
      options: locations.map((loc) => loc.name),
      editable: true,
    },
    { key: "notes", label: "Notes", type: "text", editable: true },
  ];
  const boxes = locations
    .map((loc) => {
      const locItems = items.filter((item) => item.storage_location === loc.name);
      return `
        <div class="day-box">
          <h3>
            <span>${escapeHtml(loc.name)} <small>(${locItems.length})</small></span>
            <span style="display:flex;gap:8px">
              <button class="ghost" data-add-mould="${escapeHtml(loc.name)}" style="min-height:36px">${icon("plus", 16)} Add</button>
              <button class="icon ghost" data-del-loc="${loc.id}" title="Delete location (only if empty)">${icon("x", 16)}</button>
            </span>
          </h3>
          <div data-mould-table="${escapeHtml(loc.name)}"></div>
        </div>`;
    })
    .join("");
  const unmatched = items.filter((item) => !locations.some((loc) => loc.name === item.storage_location));
  const unassigned = unmatched.length
    ? `
      <div class="day-box">
        <h3><span>Unassigned <small>(legacy locations)</small></span></h3>
        <div data-mould-table="unassigned"></div>
      </div>`
    : "";
  setPanel("Mould Locations", "", boxes + unassigned || `<div class="empty">No moulds registered.</div>`);
  store.renderActions(actionsEl, {
    refreshFn: renderMouldLocationsPanel,
    extraActions: `<button class="ghost" data-add-loc>${icon("plus", 18)} Add Location</button>`,
    onRendered: (el) => {
      el.querySelector("[data-add-loc]")?.addEventListener("click", async () => {
        const name = prompt("New mould location name:");
        if (!name || !name.trim()) return;
        try {
          await invoke("save_mould_location", { input: { id: null, name: name.trim() } });
          renderMouldLocationsPanel();
        } catch (error) {
          alert(String(error.message || error));
        }
      });
    },
  });
  const mountFor = (locName, locItems) =>
    mountInlineTable(app.querySelector(`[data-mould-table="${CSS.escape(locName)}"]`), store, {
      columns: locationColumns,
      rows: locItems,
      tableId: `loc-${CSS.escape(locName)}`,
      emptyText: "No moulds in this location",
      actionsEl,
      refreshFn: renderMouldLocationsPanel,
    });
  locations.forEach((loc) => mountFor(loc.name, items.filter((item) => item.storage_location === loc.name)));
  if (unmatched.length) mountFor("unassigned", unmatched);
  locations.forEach((loc) => {
    app.querySelector(`[data-add-mould="${CSS.escape(loc.name)}"]`).addEventListener("click", () => {
      store.addNew(`loc-${CSS.escape(loc.name)}`, {
        mould_name: "",
        storage_location: loc.name,
        notes: "",
      });
      app.querySelector(`[data-mould-table="${CSS.escape(loc.name)}"]`).dataset.remount = "1";
      renderMouldLocationsPanelKeepDrafts();
    });
  });
  locations.forEach((loc) => {
    app.querySelector(`[data-del-loc="${loc.id}"]`).addEventListener("click", async () => {
      if (!confirm(`Delete location "${loc.name}"? Only works when it has no moulds.`)) return;
      try {
        await invoke("delete_mould_location", { id: loc.id });
        renderMouldLocationsPanel();
      } catch (error) {
        alert(String(error.message || error));
      }
    });
  });
}

function renderMouldLocationsPanelKeepDrafts() {
  // Re-render the panel but keep the store alive by re-creating it fresh is NOT
  // wanted here; instead the draft was added to a store that is about to be
  // destroyed. Simplest correct behavior: full re-render loses nothing because
  // the add button instead appends through the store before this call.
  renderMouldLocationsPanel();
}
```

**STOP — the draft-loss problem:** the per-box Add button needs the *same* store that the mounted tables use, but the code above calls `renderMouldLocationsPanel()` which creates a new store and loses the draft. Fix it as follows — keep the store in module state:

Replace the two functions above with this corrected version:

```js
let mouldStore = null;

async function renderMouldLocationsPanel() {
  const [locations, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_inventory"),
  ]);
  const actionsEl = app.querySelector("[data-panel-actions]");
  const locationColumns = [
    { key: "mould_name", label: "Mould Name", type: "text", editable: true },
    {
      key: "storage_location",
      label: "Location",
      type: "select",
      options: locations.map((loc) => loc.name),
      editable: true,
    },
    { key: "notes", label: "Notes", type: "text", editable: true },
  ];
  const boxes = locations
    .map((loc) => {
      const locItems = items.filter((item) => item.storage_location === loc.name);
      return `
        <div class="day-box">
          <h3>
            <span>${escapeHtml(loc.name)} <small>(${locItems.length})</small></span>
            <span style="display:flex;gap:8px">
              <button class="ghost" data-add-mould="${escapeHtml(loc.name)}" style="min-height:36px">${icon("plus", 16)} Add</button>
              <button class="icon ghost" data-del-loc="${loc.id}" title="Delete location (only if empty)">${icon("x", 16)}</button>
            </span>
          </h3>
          <div data-mould-table="${escapeHtml(loc.name)}"></div>
        </div>`;
    })
    .join("");
  const unmatched = items.filter((item) => !locations.some((loc) => loc.name === item.storage_location));
  const unassigned = unmatched.length
    ? `
      <div class="day-box">
        <h3><span>Unassigned <small>(legacy locations)</small></span></h3>
        <div data-mould-table="unassigned"></div>
      </div>`
    : "";
  setPanel("Mould Locations", "", boxes + unassigned || `<div class="empty">No moulds registered.</div>`);

  if (!mouldStore) {
    mouldStore = createTableStore({
      commit: {
        add: (values) => invoke("save_mould_inventory", { input: values }),
        save: (values) => invoke("save_mould_inventory", { input: values }),
        remove: (id) => invoke("delete_mould_inventory", { id }),
      },
      onDone: () => {
        mouldStore = null;
        renderMouldLocationsPanel();
      },
    });
  }
  const store = mouldStore;
  store.renderActions(actionsEl, {
    refreshFn: renderMouldLocationsPanel,
    extraActions: `<button class="ghost" data-add-loc>${icon("plus", 18)} Add Location</button>`,
    onRendered: (el) => {
      el.querySelector("[data-add-loc]")?.addEventListener("click", async () => {
        const name = prompt("New mould location name:");
        if (!name || !name.trim()) return;
        try {
          await invoke("save_mould_location", { input: { id: null, name: name.trim() } });
          renderMouldLocationsPanel();
        } catch (error) {
          alert(String(error.message || error));
        }
      });
    },
  });
  const mountFor = (locName, locItems) =>
    mountInlineTable(app.querySelector(`[data-mould-table="${CSS.escape(locName)}"]`), store, {
      columns: locationColumns,
      rows: locItems,
      tableId: `loc-${CSS.escape(locName)}`,
      emptyText: "No moulds in this location",
      actionsEl,
      refreshFn: renderMouldLocationsPanel,
    });
  const mounted = {};
  locations.forEach((loc) => {
    mounted[loc.name] = mountFor(loc.name, items.filter((item) => item.storage_location === loc.name));
  });
  if (unmatched.length) mounted.unassigned = mountFor("unassigned", unmatched);

  locations.forEach((loc) => {
    app.querySelector(`[data-add-mould="${CSS.escape(loc.name)}"]`).addEventListener("click", () => {
      store.addNew(`loc-${CSS.escape(loc.name)}`, {
        mould_name: "",
        storage_location: loc.name,
        notes: "",
      });
      mounted[loc.name].render();
    });
  });
  locations.forEach((loc) => {
    app.querySelector(`[data-del-loc="${loc.id}"]`).addEventListener("click", async () => {
      if (!confirm(`Delete location "${loc.name}"? Only works when it has no moulds.`)) return;
      try {
        await invoke("delete_mould_location", { id: loc.id });
        renderMouldLocationsPanel();
      } catch (error) {
        alert(String(error.message || error));
      }
    });
  });
}
```

- [ ] **Step 3: Replace `renderStaffMouldView`**

Replace the function (app.js:1636-1651) with:

```js
async function renderStaffMouldView() {
  const [locations, items] = await Promise.all([
    invoke("list_mould_locations"),
    invoke("list_mould_inventory"),
  ]);
  const boxes = locations
    .map((loc) => {
      const locItems = items.filter((item) => item.storage_location === loc.name);
      return `
        <div class="day-box">
          <h3><span>${escapeHtml(loc.name)}</span><small>${locItems.length}</small></h3>
          ${table(["Mould Name", "Notes"], locItems.map((item) => ({ cells: [item.mould_name, item.notes] })))}
        </div>`;
    })
    .join("");
  const unmatched = items.filter((item) => !locations.some((loc) => loc.name === item.storage_location));
  const unassigned = unmatched.length
    ? `<div class="day-box"><h3><span>Unassigned</span></h3>${table(
        ["Mould Name", "Location", "Notes"],
        unmatched.map((item) => ({ cells: [item.mould_name, item.storage_location, item.notes] })),
      )}</div>`
    : "";
  setPanel(
    "Mould Locations (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    boxes + unassigned || `<div class="empty">No moulds registered.</div>`,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffMouldView);
}
```

- [ ] **Step 4: Verify**

Run: `node --check --input-type=module < ui/js/app.js`
Expected: no output.

- [ ] **Step 5: Manual smoke note**

Kiosk: admin → Mould Locations shows 3 seeded boxes; Add Location prompts and adds a box; per-box Add appends an editable row with the location preselected; editing the Location select moves the row between boxes after save; delete-location is blocked while moulds exist; staff (storekeeper/driver/helper) see read-only boxes.

- [ ] **Step 6: Commit**

```bash
git add ui/js/app.js
git commit -m "feat: mould locations panel with extensible sections and inline editing"
```

---

### Task 12: UI — Rates grouped by series (admin inline + staff RO)

**Files:**
- Modify: `ui/js/app.js` (replace `renderRatesPanel` at 664-728 and `renderStaffRates` at 1622-1634; remove `selectedRate` from state at line 27)

**Interfaces:**
- Consumes: `createTableStore`/`mountInlineTable` (Task 9), `list_cornice_rates`/`save_cornice_rate`/`delete_cornice_rate` (Task 5).
- Produces: `renderRatesPanel()` — add-form on top (spec §3.4: "Edit form stays on top") + one `<h3>` + inline table per series (sorted); editing a Series cell re-groups after save. `renderStaffRates()` — same grouping, Model/Unit only (value hidden).

- [ ] **Step 1: Remove `selectedRate` from state**

In `ui/js/app.js` `state` (line 27), delete the line `selectedRate: null,` (the form becomes add-only; row editing moves to inline cells).

- [ ] **Step 2: Replace `renderRatesPanel`**

Replace the entire function (app.js:664-728) with:

```js
let ratesStore = null;

async function renderRatesPanel() {
  const rates = await invoke("list_cornice_rates");
  const groups = {};
  for (const rate of rates) {
    const series = rate.series || "(no series)";
    (groups[series] ||= []).push(rate);
  }
  const seriesNames = Object.keys(groups).sort();
  setPanel(
    "Cornice Rates",
    "",
    `
      <form class="form-grid" data-rate-form>
        <label>Series<input name="series" /></label>
        <label>Model<input name="model" /></label>
        <label>Unit Text<input name="unit_text" /></label>
        <label>Unit Value<input name="unit_value" type="number" step="0.01" /></label>
        <label class="check"><input type="checkbox" name="is_confidential" checked /> Confidential</label>
        <div class="wide panel-actions"><button class="primary" type="submit">Add Rate</button></div>
      </form>
      ${
        seriesNames
          .map(
            (series) => `
      <h3>${escapeHtml(series)}</h3>
      <div data-rate-group="${escapeHtml(series)}"></div>`,
          )
          .join("") || `<div class="empty">No rates yet.</div>`
      }
    `,
  );
  app.querySelector("[data-rate-form]").addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await invoke("save_cornice_rate", {
      input: {
        id: null,
        series: form.get("series"),
        model: form.get("model"),
        unit_text: form.get("unit_text"),
        unit_value: form.get("unit_value") ? Number(form.get("unit_value")) : null,
        is_confidential: form.get("is_confidential") === "on",
      },
    });
    renderRatesPanel();
  });

  if (!ratesStore) {
    ratesStore = createTableStore({
      commit: {
        add: (values) => invoke("save_cornice_rate", { input: { id: null, ...values } }),
        save: (values) => invoke("save_cornice_rate", { input: values }),
        remove: (id) => invoke("delete_cornice_rate", { id }),
      },
      onDone: () => {
        ratesStore = null;
        renderRatesPanel();
      },
    });
  }
  const store = ratesStore;
  const mountedRates = {};
  const firstSeries = seriesNames[0] || "";
  seriesNames.forEach((series) => {
    mountedRates[series] = mountInlineTable(
      app.querySelector(`[data-rate-group="${CSS.escape(series)}"]`),
      store,
      {
        columns: [
          { key: "series", label: "Series", type: "text", editable: true },
          { key: "model", label: "Model", type: "text", editable: true },
          { key: "unit_text", label: "Unit", type: "text", editable: true },
          { key: "unit_value", label: "Value", type: "number", editable: true, align: "right" },
          { key: "is_confidential", label: "Confidential", type: "bool", editable: true },
        ],
        rows: groups[series],
        tableId: `series-${CSS.escape(series)}`,
        emptyText: "No models in this series",
        actionsEl: app.querySelector("[data-panel-actions]"),
        refreshFn: renderRatesPanel,
        extraActions: `<button class="ghost" data-add-rate>${icon("plus", 18)} Add</button>`,
        onActionsRendered: (el) => {
          el.querySelector("[data-add-rate]")?.addEventListener("click", () => {
            store.addNew(`series-${CSS.escape(firstSeries)}`, {
              series: firstSeries,
              model: "",
              unit_text: "",
              unit_value: null,
              is_confidential: true,
            });
            mountedRates[firstSeries]?.render();
          });
        },
      },
    );
  });
}
```

NOTE: `extraActions`/`onActionsRendered` are passed through `mountInlineTable`'s config (NOT a direct `store.renderActions` call) so the component's internal `refreshActions()` re-renders the actions with the Add button intact after every table render. `mountedRates` is declared before the mount loop; the Add handler only runs on click (after the loop completes).

- [ ] **Step 3: Replace `renderStaffRates`**

Replace the function (app.js:1622-1634) with:

```js
async function renderStaffRates() {
  const rates = await invoke("list_cornice_rates");
  const groups = {};
  for (const rate of rates) {
    const series = rate.series || "(no series)";
    (groups[series] ||= []).push(rate);
  }
  const body =
    Object.keys(groups)
      .sort()
      .map(
        (series) => `
      <h3>${escapeHtml(series)}</h3>
      ${table(["Model", "Unit"], groups[series].map((rate) => ({ cells: [rate.model, rate.unit_text] })))}`,
      )
      .join("") || `<div class="empty">No rates yet.</div>`;
  setPanel(
    "Cornice Rates (Read-Only)",
    `<button class="ghost" data-refresh>Refresh</button>`,
    body,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderStaffRates);
}
```

- [ ] **Step 4: Verify**

Run: `node --check --input-type=module < ui/js/app.js`
Expected: no output.

- [ ] **Step 5: Manual smoke note**

Kiosk: admin → Cornice Rates shows the add-form on top and one section per series (sorted); Value column right-aligned with tabular numerals; click a cell to edit; editing a Series cell moves the row to another group after Save; Add appends a draft row to the first series; Save/Discard/Refresh swap in the actions row; staff (helper with `cornice_rates_view`) see sections with Model/Unit only.

- [ ] **Step 6: Commit**

```bash
git add ui/js/app.js
git commit -m "feat: rates grouped by series with inline editing"
```

---
### Task 13: UI — Database storage line + boot disk check

**Files:**
- Modify: `ui/js/app.js` (add `fmtBytes` helper; `loadStatus` at 48-59; `renderDatabasePanel` at 836-948)

**Interfaces:**
- Consumes: `storage_status` (Task 4).
- Produces: `fmtBytes(bytes)` helper; storage line in the database panel body; boot-time `storage_status` call (triggers the backend >90% `disk_space` alert).

- [ ] **Step 1: Add `fmtBytes` helper**

In `ui/js/app.js`, after `cellLooksHtml` (line ~1826), add:

```js
function fmtBytes(bytes) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Number(bytes);
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value >= 100 || index === 0 ? Math.round(value) : value.toFixed(1)} ${units[index]}`;
}
```

- [ ] **Step 2: Boot check in `loadStatus`**

In `loadStatus()` (app.js:48-59), after `state.status = await invoke("app_status");` add (fire-and-forget; the backend inserts the red `disk_space` alert when usage >90%):

```js
    invoke("storage_status").catch(() => {});
```

- [ ] **Step 3: Storage line in `renderDatabasePanel`**

In `renderDatabasePanel` (app.js:836), after `const data = await invoke("list_admin_table_rows", { table: state.adminDbTable });` add:

```js
  const storage = await invoke("storage_status");
  const storageLine = `<div class="message">DB ${fmtBytes(storage.db_size_bytes)} · Disk ${Math.round(storage.disk_used_pct)}% used · ${fmtBytes(storage.disk_free_bytes)} free</div>`;
```

Then change the `setPanel(...)` body argument (the `readOnly ? ... : ...` expression starting at line ~865) to prepend the line:

```js
    storageLine +
      (readOnly
        ? `...existing read-only body...`
        : `...existing editable body...`),
```

(i.e. wrap the existing ternary in `storageLine + ( ... )` — do not retype the bodies, just add the prefix and adjust indentation).

- [ ] **Step 4: Verify**

Run: `node --check --input-type=module < ui/js/app.js`
Expected: no output.

- [ ] **Step 5: Manual smoke note**

Kiosk: admin → Database shows `DB <size> · Disk <pct>% used · <free> free` under the header. To test the alert, temporarily lower the threshold in `storage_status` (commands.rs) to e.g. `1.0`, rebuild, boot the app, and confirm a red `disk_space` notification appears in Alerts exactly once (dedupe: reboot does not add a second one). Revert the threshold afterwards.

- [ ] **Step 6: Commit**

```bash
git add ui/js/app.js
git commit -m "feat: database panel storage line and boot-time disk check"
```

---

### Task 14: UI — Staff cornice week/day log with inline edit/delete

**Files:**
- Modify: `ui/js/app.js` (replace `renderStaffCornice` at 1358-1459; add shared helpers `parsePrevValues`, `corniceLogCellHtml`, `shiftIso`, `bindCorniceModelSearch`; add `staffLogStore` module var)

**Interfaces:**
- Consumes: `list_cornice_logs` (pass `weekStart: null` — the command filters `? IS NULL OR week_start = ?`, so null returns all weeks), `add_cornice_log`, `update_cornice_log`/`delete_cornice_log` (Task 6), `search_cornice_rates`, `createTableStore`/`mountInlineTable` (Task 9), `weekStartIso()`, `todayIso()`.
- Produces: `renderStaffCornice()` — week boxes (most recent first) containing day boxes (most recent first); the current week + today's day box always exist so "+ Add" has a home; Model cell has a datalist + fuzzy hint; pending entries render `<s>old</s> → new`; amended entries get a ✎ cue; staged deletes render struck through with a "pending deletion" tag. Shared helpers `parsePrevValues(log)` / `corniceLogCellHtml(log, key)` are reused by Task 15.

- [ ] **Step 1: Add shared helpers**

In `ui/js/app.js`, after `fmtBytes` (Task 13), add:

```js
function shiftIso(iso, days) {
  const [year, month, day] = iso.split("-").map(Number);
  const date = new Date(year, month - 1, day + days);
  const pad = (value) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function parsePrevValues(log) {
  if (!log.prev_values) return null;
  try {
    const parsed = JSON.parse(log.prev_values);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

// Raw-HTML cell for cornice log Model/Lengths cells (pending old→new, ✎ cue, staged delete).
// Returns null → caller falls back to the plain escaped value.
function corniceLogCellHtml(log, key) {
  const prev = parsePrevValues(log);
  if (prev && prev.deleted) {
    return `<span class="old-new"><s>${escapeHtml(log.model)}</s> <span class="tag warn">pending deletion</span></span>`;
  }
  let cell;
  if (prev && prev[key] !== undefined && String(prev[key]) !== String(log[key])) {
    cell = `<span class="old-new"><s>${escapeHtml(prev[key])}</s> → ${escapeHtml(log[key])}</span>`;
  } else {
    cell = escapeHtml(log[key]);
  }
  if (log.amended_at) {
    return `<span class="amended-model">${cell}<span class="amended" title="Amended ${escapeHtml(log.amended_at)} by ${escapeHtml(log.amended_by || "")}">✎</span></span>`;
  }
  return key === "model" ? null : cell;
}
```

NOTE: for non-pending, non-amended rows the Model cell returns `null` (plain escaped render); Lengths always returns the plain value (it never carries the ✎ cue).

- [ ] **Step 2: Replace `renderStaffCornice`**

Replace the entire function (app.js:1358-1459) with:

```js
let staffLogStore = null;
let corniceModelSearchTimer = null;

function bindCorniceModelSearch() {
  const panelBody = app.querySelector("[data-panel-body]");
  if (!panelBody || panelBody.dataset.corniceSearchBound) return;
  panelBody.dataset.corniceSearchBound = "1";
  panelBody.addEventListener("input", (event) => {
    const input = event.target;
    if (!input.matches('input[data-key="model"]') || state.staffView !== "cornice") return;
    const query = input.value.trim();
    clearTimeout(corniceModelSearchTimer);
    if (query.length < 2) {
      app.querySelector("#cornice-search-results")?.style.setProperty("display", "none");
      return;
    }
    corniceModelSearchTimer = setTimeout(async () => {
      try {
        const resp = await invoke("search_cornice_rates", { request: { query } });
        state.corniceRateMatches = resp.matches || [];
        const box = app.querySelector("#cornice-search-results");
        if (!box) return;
        box.textContent = (resp.matches || []).length
          ? `${resp.matches.length} match(es) — pick from the dropdown or keep typing.`
          : "No match found — will be logged as unknown/custom.";
        box.style.display = "block";
      } catch {
        /* ignore */
      }
    }, 200);
  });
}

async function renderStaffCornice() {
  const logs = await invoke("list_cornice_logs", {
    employeeId: state.currentStaff.id,
    date: null,
    weekStart: null,
  });
  const today = todayIso();
  const currentWeek = weekStartIso();

  const weeks = new Map();
  for (const log of logs) {
    if (!weeks.has(log.week_start)) weeks.set(log.week_start, new Map());
    const days = weeks.get(log.week_start);
    if (!days.has(log.log_date)) days.set(log.log_date, []);
    days.get(log.log_date).push(log);
  }
  if (!weeks.has(currentWeek)) weeks.set(currentWeek, new Map());
  const currentDays = weeks.get(currentWeek);
  if (!currentDays.has(today)) currentDays.set(today, []);

  const weekNames = [...weeks.keys()].sort().reverse();
  const body = weekNames
    .map((weekStart) => {
      const days = weeks.get(weekStart);
      const dayNames = [...days.keys()].sort().reverse();
      const weekTotal = [...days.values()].flat().reduce((sum, log) => sum + log.total_units, 0);
      const dayBoxes = dayNames
        .map((date) => {
          const dayLogs = days.get(date);
          const dayTotal = dayLogs.reduce((sum, log) => sum + log.total_units, 0);
          return `
            <div class="day-box">
              <h3><span>${escapeHtml(date)}</span><small>${dayTotal.toFixed(2)} units</small></h3>
              <div data-day-table="${escapeHtml(date)}"></div>
            </div>`;
        })
        .join("");
      return `
        <div class="week-box">
          <h3><span>${escapeHtml(weekStart)} – ${escapeHtml(shiftIso(weekStart, 6))}</span><small>${weekTotal.toFixed(2)} units</small></h3>
          ${dayBoxes}
        </div>`;
    })
    .join("");
  setPanel(
    "Cornice Log",
    "",
    `
      <datalist id="staff-cornice-models"></datalist>
      <div id="cornice-search-results" class="message" style="display:none;margin-bottom:12px;font-size:0.9em;"></div>
      ${body || `<div class="empty">No log entries yet.</div>`}
    `,
  );

  try {
    const resp = await invoke("search_cornice_rates", { request: { query: "" } });
    app.querySelector("#staff-cornice-models").innerHTML = (resp.matches || [])
      .map(
        (match) =>
          `<option value="${escapeHtml(match.model)}">${escapeHtml(match.series ? `${match.series} ` : "")}${escapeHtml(match.model)} (${match.unit_text})</option>`,
      )
      .join("");
  } catch {
    /* ignore */
  }
  bindCorniceModelSearch();

  if (!staffLogStore) {
    staffLogStore = createTableStore({
      commit: {
        add: (values) => {
          const match = state.corniceRateMatches.find(
            (item) => item.model.toLowerCase() === String(values.model).trim().toLowerCase(),
          );
          return invoke("add_cornice_log", {
            input: {
              employee_id: state.currentStaff.id,
              log_date: todayIso(),
              series: match?.series || "",
              model: values.model,
              lengths: values.lengths,
            },
          });
        },
        save: (values) =>
          invoke("update_cornice_log", {
            input: {
              id: values.id,
              actor_id: state.currentStaff.id,
              series: values.series,
              model: values.model,
              lengths: values.lengths,
            },
          }),
        remove: (id) => invoke("delete_cornice_log", { id, actorId: state.currentStaff.id }),
      },
      onDone: () => {
        staffLogStore = null;
        renderStaffCornice();
      },
    });
  }
  const store = staffLogStore;
  const mountedDays = {};
  weekNames.forEach((weekStart) => {
    const days = weeks.get(weekStart);
    [...days.keys()].sort().reverse().forEach((date) => {
      mountedDays[date] = mountInlineTable(
        app.querySelector(`[data-day-table="${CSS.escape(date)}"]`),
        store,
        {
          columns: [
            {
              key: "model",
              label: "Model",
              type: "text",
              editable: true,
              list: "staff-cornice-models",
              cellHtml: (log, col) => corniceLogCellHtml(log, col.key),
            },
            {
              key: "lengths",
              label: "Lengths",
              type: "number",
              editable: true,
              align: "right",
              cellHtml: (log, col) => corniceLogCellHtml(log, col.key),
            },
            {
              key: "unit_text",
              label: "Unit",
              type: "text",
              editable: false,
              cellHtml: (log) => escapeHtml(log.unit_text || "Custom"),
            },
            {
              key: "total_units",
              label: "Units",
              type: "number",
              editable: false,
              align: "right",
              cellHtml: (log) => log.total_units.toFixed(2),
            },
          ],
          rows: days.get(date),
          tableId: `day-${CSS.escape(date)}`,
          emptyText: "No entries",
          rowClass: (log) => (parsePrevValues(log)?.deleted ? "staged-delete" : ""),
          actionsEl: app.querySelector("[data-panel-actions]"),
          refreshFn: renderStaffCornice,
          extraActions: `<button class="ghost" data-add-log>${icon("plus", 18)} Add</button>`,
          onActionsRendered: (el) => {
            el.querySelector("[data-add-log]")?.addEventListener("click", () => {
              store.addNew(`day-${CSS.escape(today)}`, {
                id: null,
                series: "",
                model: "",
                lengths: 0,
                unit_text: "",
                total_units: 0,
              });
              mountedDays[today]?.render();
            });
          },
        },
      );
    });
  });
}
```

NOTE: the old top form (series/model/lengths + datalist + fuzzy box, app.js:1368-1458) is replaced by the inline "+ Add" row; the datalist is populated once with all models (`query: ""`) and the fuzzy hint box is re-implemented via `bindCorniceModelSearch()` (delegated `input` listener on the persistent `[data-panel-body]` element, bound once via a dataset flag, guarded by `state.staffView === "cornice"`). `state.corniceRateMatches` is still used for series auto-fill on add.

- [ ] **Step 3: Verify**

Run: `node --check --input-type=module < ui/js/app.js`
Expected: no output.

- [ ] **Step 4: Manual smoke note**

Kiosk (staff, cornice_hand): Cornice tab shows week boxes (most recent first) with day boxes inside; today's box exists even with no entries; Add appends an editable row to today; typing in Model shows the datalist + fuzzy hint; Save commits (add_cornice_log with auto-filled series); editing a Lengths cell marks the row dirty; deleting a same-day entry removes it immediately; editing/deleting an older-day entry shows `<s>old</s> → new` / struck-through "pending deletion" and raises the yellow alert; ✎ cue appears on amended rows.

- [ ] **Step 5: Commit**

```bash
git add ui/js/app.js
git commit -m "feat: staff cornice week/day log with inline edit, delete, and pending old→new rendering"
```

---

### Task 15: UI — Admin Daily Logs: pending old→new + Approve

**Files:**
- Modify: `ui/js/app.js` (replace `renderLogsPanel` at 798-834; extend `cellLooksHtml` at 1823-1826)

**Interfaces:**
- Consumes: `list_cornice_logs` (call twice: `weekStart: weekStartIso()` for the week table is replaced by one `weekStart: null` call + client-side split), `approve_cornice_log` (Task 6), `corniceLogCellHtml`/`parsePrevValues` (Task 14), `table()` helper.
- Produces: admin Daily Logs cornice table with pending rendering (old→new, ✎, staged delete) + an Approve button on every pending row, including a "Pending Approval (previous weeks)" section so older-day edits are always reachable.

- [ ] **Step 1: Extend `cellLooksHtml`**

Replace `cellLooksHtml` (app.js:1823-1826) so the `table()` helper renders the pending/amended raw-HTML cells:

```js
function cellLooksHtml(value) {
  const v = typeof value === "string" ? value.trim() : "";
  return (
    v.startsWith("<button") ||
    v.startsWith('<span class="tag') ||
    v.startsWith('<span class="old-new') ||
    v.startsWith('<span class="amended-model')
  );
}
```

- [ ] **Step 2: Replace `renderLogsPanel`**

Replace the entire function (app.js:798-834) with:

```js
async function renderLogsPanel() {
  const allLogs = await invoke("list_cornice_logs", {
    employeeId: null,
    date: null,
    weekStart: null,
  });
  const weekLogs = allLogs.filter((log) => log.week_start === weekStartIso());
  const pendingOlder = allLogs.filter(
    (log) => log.needs_admin_review && log.week_start !== weekStartIso(),
  );
  const production = await invoke("list_production_logs", { employeeId: null, date: null });
  const logColumns = ["Date", "Employee", "Model", "Lengths", "Units", "Week Units", ""];
  const logRow = (log) => ({
    review: log.needs_admin_review,
    cells: [
      log.log_date,
      log.employee_name,
      corniceLogCellHtml(log, "model") ?? log.model,
      corniceLogCellHtml(log, "lengths") ?? String(log.lengths),
      log.total_units.toFixed(2),
      log.weekly_units.toFixed(2),
      log.needs_admin_review
        ? `<button class="ghost" data-approve-log="${log.id}">Approve</button>`
        : "",
    ],
  });
  setPanel(
    "Daily Logs",
    `<button class="ghost" data-refresh>Refresh</button>`,
    `
      <h3>Cornice Units This Week</h3>
      ${table(logColumns, weekLogs.map(logRow))}
      ${
        pendingOlder.length
          ? `<h3>Pending Approval (previous weeks)</h3>${table(logColumns, pendingOlder.map(logRow))}`
          : ""
      }
      <h3>Production Logs</h3>
      ${table(
        ["Date", "Employee", "Item", "Quantity", "Notes"],
        production.map((log) => ({
          cells: [log.log_date, log.employee_name, log.item, log.quantity, log.notes],
        })),
      )}
    `,
  );
  app.querySelector("[data-refresh]").addEventListener("click", renderLogsPanel);
  app.querySelectorAll("[data-approve-log]").forEach((button) => {
    button.addEventListener("click", async () => {
      await invoke("approve_cornice_log", { id: Number(button.dataset.approveLog) });
      renderLogsPanel();
    });
  });
}
```

- [ ] **Step 3: Verify**

Run: `node --check --input-type=module < ui/js/app.js`
Expected: no output.

- [ ] **Step 4: Manual smoke note**

Kiosk: staff edits an older-day entry → admin Daily Logs shows the row with `<s>old</s> → new` (or struck-through "pending deletion") under "Pending Approval (previous weeks)"; Approve clears the pending state (staged delete removes the row), resolves the yellow alert, and the row shows only the new value with ✎.

- [ ] **Step 5: Commit**

```bash
git add ui/js/app.js
git commit -m "feat: admin daily logs pending old→new rendering and approve flow"
```

---

### Task 16: UI — Clock in/out: smart button, confirm modals, no re-auth

**Files:**
- Modify: `ui/js/auth.js` (delete `chooseClockAction` at 239-271; add `confirmModal`)
- Modify: `ui/js/app.js` (import line 9; `state` at 14-35; staff login at 261-265; replace `renderStaffClock` at 1310-1356)

**Interfaces:**
- Consumes: `get_clock_status` (Task 7), `record_clock_event`, `confirmModal` (new), `state.currentStaff`.
- Produces: `state.sessionSource` (login source reused for clock events); smart Clock button label from `today_state`; `confirmModal({ title, body, warning, confirmLabel, cancelLabel })` → `Promise<void>` that rejects with `Error("Cancelled.")` on cancel/close.

- [ ] **Step 1: Replace `chooseClockAction` with `confirmModal` in `auth.js`**

Delete `chooseClockAction` (auth.js:239-271) and add:

```js
export function confirmModal({
  title,
  body = "",
  warning = null,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
}) {
  return new Promise((resolve, reject) => {
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            ${body ? `<p>${body}</p>` : ""}
            ${warning ? `<div class="scan-status warn">${escapeHtml(warning)}</div>` : ""}
          </div>
          <footer>
            <button class="ghost" data-cancel>${escapeHtml(cancelLabel)}</button>
            <button class="primary" data-confirm>${escapeHtml(confirmLabel)}</button>
          </footer>
        </section>
      </div>
    `;
    const cancel = () => {
      closeModal();
      reject(new Error("Cancelled."));
    };
    modalRoot.querySelector("[data-close]").addEventListener("click", cancel);
    modalRoot.querySelector("[data-cancel]").addEventListener("click", cancel);
    modalRoot.querySelector("[data-confirm]").addEventListener("click", () => {
      closeModal();
      resolve(true);
    });
  });
}
```

(`escapeHtml`, `icon`, `modalRoot`, `closeModal` are already in scope in auth.js.)

- [ ] **Step 2: Track login source in `app.js`**

a) Import: change line 9 to:

```js
import { confirmModal, requestAuth } from "./auth.js";
```

b) In `state` (app.js:14-35), add after `currentStaff: null,`:

```js
  sessionSource: null,
```

c) In the staff login handler (app.js:261-263), after `state.currentStaff = response.employee;` add:

```js
        state.sessionSource = response.source;
```

- [ ] **Step 3: Replace `renderStaffClock`**

Replace the entire function (app.js:1310-1356) with:

```js
async function renderStaffClock(message = "") {
  const [events, status] = await Promise.all([
    invoke("list_clock_events", { date: todayIso() }),
    invoke("get_clock_status", { employeeId: state.currentStaff.id }),
  ]);
  const nextAction = status.today_state === "in" ? "clock_out" : "clock_in";
  setPanel(
    "Clock",
    `<button class="primary" data-clock>${nextAction === "clock_in" ? "Clock In" : "Clock Out"}</button>`,
    `
      ${message ? `<div class="message">${escapeHtml(message)}</div>` : ""}
      ${table(
        ["Time", "Employee", "Action", "Note"],
        events
          .filter((event) => event.employee_id === state.currentStaff.id)
          .map((event) => ({
            review: event.needs_admin_review,
            cells: [
              event.timestamp.replace("T", " "),
              event.employee_name,
              formatAction(event.action),
              event.note,
            ],
          })),
      )}
    `,
  );
  app.querySelector("[data-clock]").addEventListener("click", async (event) => {
    setBusy(event.currentTarget);
    try {
      const warning =
        nextAction === "clock_in" && status.missed_yesterday_clock_out
          ? "You didn't clock out yesterday."
          : null;
      await confirmModal({
        title: nextAction === "clock_in" ? "Clock in now?" : "Clock out now?",
        confirmLabel: nextAction === "clock_in" ? "Clock In" : "Clock Out",
        warning,
      });
      const result = await invoke("record_clock_event", {
        request: {
          employee_id: state.currentStaff.id,
          action: nextAction,
          source: state.sessionSource || "password",
        },
      });
      renderStaffClock(`${formatAction(result.action)} recorded at ${result.timestamp.slice(11)}`);
    } catch (error) {
      if (String(error.message || error) !== "Cancelled.") {
        renderStaffClock(String(error.message || error));
      }
    } finally {
      setBusy(event.currentTarget, false);
    }
  });
}
```

NOTE: the `requestAuth()` + `chooseClockAction()` calls are gone — no re-auth; the button label is driven by `get_clock_status`; the missed-clock-out warning line reuses the `.scan-status.warn` style; the red `missing_clock_out` admin alert still fires server-side inside `record_clock_event` via `refresh_attendance_issues`.

- [ ] **Step 4: Verify**

Run: `node --check --input-type=module < ui/js/auth.js && node --check --input-type=module < ui/js/app.js`
Expected: no output.

- [ ] **Step 5: Manual smoke note**

Kiosk (staff): Clock tab shows "Clock In" initially; click → "Clock in now?" confirm modal (no fingerprint/password prompt); confirm → event recorded with the login source; button flips to "Clock Out"; after clock out it shows "Clock In" (re-entry). To test the warning: delete today's clock_out in the DB (or clock in yesterday with no out), reload, clock in → modal shows "You didn't clock out yesterday." and the red admin alert appears.

- [ ] **Step 6: Commit**

```bash
git add ui/js/auth.js ui/js/app.js
git commit -m "feat: clock in/out confirm modals with smart button and no re-auth"
```

---

### Task 17: Final verification + handoff

**Files:** none (verification only)

- [ ] **Step 1: Backend check + tests**

Run: `cd src-tauri && cargo check && cargo test 2>&1 | tail -15`
Expected: check passes; all tests pass (including Task 3 migration tests and Task 4 `storage_tests`).

- [ ] **Step 2: JS syntax check**

Run: `for f in ui/js/app.js ui/js/table.js ui/js/auth.js ui/js/api.js ui/js/icons.js; do node --check --input-type=module < "$f" || echo "FAIL $f"; done`
Expected: no FAIL lines.

- [ ] **Step 3: Migration idempotency**

Start the app twice against the existing `hps.db` (or run the migrate path twice in a test). Verify:
- `SELECT item_type, model, COUNT(*) FROM stock_items GROUP BY 1, 2 HAVING COUNT(*) > 1;` → empty (no duplicate cornice rows from repeated copies).
- `SELECT COUNT(*) FROM mould_locations;` → 3 (seed not duplicated).
- No migration errors in the log.

- [ ] **Step 4: Full UI smoke (spec Verification section)**

Home clock (uniform digit color, 100px logo, larger date), session pill pulses red <10s, auth modal (centered glyph, no bracket bar), tab clicks don't rebuild the shell (DevTools: side-nav DOM nodes persist), Stocks filter + inline add/edit/delete/save/discard, Mould Locations add-location + per-section rows + Unassigned section, Rates grouping + series cell edit, Database storage line + disk alert at >90% (threshold lowered temporarily), staff log week/day boxes + `<s>old</s> → new` rendering + approve flow, clock in/out confirm modals + no re-auth + missed-clock-out warning.

- [ ] **Step 5: Refresh knowledge graph**

Run: `graphify update .`

- [ ] **Step 6: Remind user about the out-of-scope fingerprint bug**

Tell the user (spec §6): the fingerprint cross-match issue is still open — enrolling a new employee breaks matching for previously enrolled fingers (identify returns the other employee). Relevant git history: `174e7eb` (multi-template), `ea6d277` (polling similarity), `8951f81` (auto-retry), `63a73e3` (kills old helper). Threshold 24 / distance_match 0.80 already tried without success. Suggest investigating as a follow-up task.

- [ ] **Step 7: Stray changes**

Run: `git status --short` — if anything is uncommitted (e.g. the temporary threshold revert from Step 4), commit it with a message describing the change.
