# Hopkins Inventory Management Software - Agent Reference

## Project Snapshot

Hopkins Plaster Studio kiosk + inventory system has two codebases:

| Version | Path | Role |
|---|---|---|
| Rust/Tauri (active) | `src-tauri/`, `ui/` | Production app in active development |
| Python legacy (reference) | `Hopkins-Inventory-Management/` | Historical reference for behavior and helper flow |

The fingerprint stack is shared via the C helper binary:
`libfprint-CS9711/build/examples/employee-clock-helper`.

---

## Current Runtime Architecture

### Backend (Rust/Tauri)
- `src-tauri/src/commands/`: Tauri commands, split by domain. `mod.rs` holds shared
  helpers (`CommandResult`, admin-table commands, `app_status`, storage) and re-exports
  everything so `lib.rs` is untouched. Domain modules: `employees.rs`, `stock.rs`,
  `rates.rs`, `logs.rs`, `alerts.rs`, `moulds.rs`, `payroll.rs`, `dispatch.rs`.
- `src-tauri/src/fingerprint.rs`: helper discovery, spawn, protocol parsing, template import/export
- `src-tauri/src/db/`: SQLite layer, split into `mod.rs` (AppState, AppPaths, job structs,
  employee/notification query helpers), `migrate.rs` (schema DDL + column/data/cornice-unit
  migrations), `seed.rs` (asset/employee/rate/stock seeding, legacy imports), `util.rs`
  (pure date/password/unit/csv helpers). Glob re-exports keep the `crate::db::` API stable.
- `src-tauri/src/backup.rs`: VACUUM INTO backups, weekly/monthly scheduler, retention, `exit_kiosk`
- `src-tauri/src/models.rs`: command response/request models

### Frontend (Web UI)
- `ui/js/app.js`: orchestrator — session management, navigation, admin/staff tab dispatch
- `ui/js/core.js`: shared `state`, `app` root, shell `table`/`setPanel` helpers, date/format/db utilities
- `ui/js/panels/`: one module per screen — `admin-alerts`, `admin-employees`, `admin-stock`,
  `admin-rates`, `admin-logs`, `admin-about`, `admin-payroll`, `admin-moulds`,
  `admin-dispatch`, `staff`
- `ui/js/modals.js`: generic modal primitives (`alertModal`, `promptModal`, `confirmModal`, `closeModal`)
- `ui/js/auth.js`: fingerprint/password auth + enrollment flows
- `ui/js/api.js`: Tauri invoke wrappers
- `ui/js/table.js`, `ui/js/rates-cards.js`, `ui/js/search.js`, `ui/js/icons.js`: shared components

### Persistence
- SQLite DB: `hps.db` (repo root)
- Fingerprint temp/cache files: `data/fingerprints/`
  - Enrollment writes `<employee_id>.fpdata` (single template) and `<employee_id>.fpimg` (bundle of the 15 sub-print images), persists both to SQLite, removes temp files.
  - Identify exports templates (and image bundles) from SQLite to `data/fingerprints/`, then clears cache after scan.
- `fingerprint_templates` table: one row per `employee_id` (single template). Columns: `employee_id` (PK), `finger` (the enrolled finger), `template` (BLOB), `images` (BLOB, optional `.fpimg` bundle used for alignment hints), `updated_at`.
- Database backups: `data/backup/weekly/` and `data/backup/monthly/` (see Database Backup & Restore).

---

## Database Backup & Restore

Everything lives in `hps.db` (employees, fingerprint templates, payroll, logs, stock, media BLOBs).
`data/fingerprints/` is ephemeral cache — never backed up.

### Mechanism
`VACUUM INTO '<path>'` — SQLite's consistent online copy, WAL-safe, no extra dependencies.
Implemented in `src-tauri/src/backup.rs` (`backup_now`).

### Layout, schedule, retention
| Tier | Location | When | Retention |
|---|---|---|---|
| weekly | `data/backup/weekly/hps-YYYYMMDD-HHMMSS.db` | Sundays ≥12:00, if no backup for the current ISO week | newest 4 |
| monthly | `data/backup/monthly/hps-YYYYMMDD-HHMMSS.db` | first Sunday of month ≥12:00, if no backup for the current calendar month | newest 12 |

- A scheduler thread (`spawn_scheduler`, started in `lib.rs` setup) ticks every 60s; the first tick
  runs at startup, so a missed Sunday (kiosk powered off) is caught up on next start.
- Manual "Backup now" (admin About panel) always writes to `weekly/` and counts against the 4-file cap.
- A failed backup inserts an `admin_notifications` row (severity `yellow`, kind `backup`) — visible
  in the Admin Alerts tab.

### Commands
- `create_database_backup` → manual backup into `weekly/`
- `backup_status` → `{ weekly: {count, latest}, monthly: {count, latest} }` for the About panel
- `exit_kiosk` → fingerprint-gated shutdown (see below)

### Restore (manual, offline)
1. Stop the app (kill the process).
2. `cp data/backup/weekly/hps-<timestamp>.db hps.db`
3. Delete stale WAL sidecars: `rm -f hps.db-wal hps.db-shm`
4. Start the app, then verify: `sqlite3 hps.db "PRAGMA integrity_check;"` → expect `ok`.

No source access or recompile is needed — the binary reads whatever `hps.db` sits next to it.
Out of scope: offsite/remote sync (single kiosk, no network guarantee per spec).

### Exit Kiosk
The kiosk locks itself: `RunEvent::ExitRequested` calls `prevent_exit()` unless
`AppState.allow_exit` is set. Only the `exit_kiosk` command sets it — the admin About panel's
"Exit Kiosk" button first runs `requestAuth({ requireAdmin: true })` (same admin fingerprint modal
as entering admin), then invokes `exit_kiosk` → `app.exit(0)`. Window min/max/close buttons are
slated for removal in the final release.

---

## Fingerprint Device Context

- Hardware: WA28 reader (CS9711 chipset)
- Typical USB IDs: `2541:0236` / `2541:9711`
- Helper protocol lines:
  - `DEVICE|...`
  - `ENROLL_STAGES|N`
  - `READY|...`
  - `PROGRESS|completed|total`
  - `RETRY|...`
  - `ERROR|...`
  - `ENROLLED|...`
  - `MATCH|...`
  - `NO_MATCH`
  - `ATTEMPT|N|3|waiting` (identify: which scan attempt is in progress)
  - `BEST|<employee_id>|<score>` (identify: closest employee + max SIFT score, emitted before `NO_MATCH`)
  - `HINT|<employee_id>|<ncc_pct>|<slide_x_mm>|<slide_y_mm>` (identify: NCC alignment of the scanned finger vs the enrolled sub-print images; signed slide, +x right / +y down; only when the employee has a `.fpimg` bundle)

Only these protocol lines are consumed by Rust from helper output.

---

## Fingerprint Matching Algorithm (SIGFM)

The CS9711 driver uses the **SIGFM** algorithm (`libfprint/sigfm/sigfm.cpp`) — a SIFT-based
minutiae matcher. Understanding its parameters is essential for tuning accuracy.

### How matching works
1. **Keypoint extraction**: SIFT detects keypoints + descriptors in the live scan image.
2. **Ratio test**: A keypoint match is accepted only if `best_distance < distance_match × second_best_distance`.
3. **Geometric consistency**: For all matched keypoint pairs, vector lengths must agree within `length_match` (5%) and angles within `angle_match` (5%).
4. **Score**: Count of consistent angle pairs. Must be ≥ `score_threshold` to be a match.

### Tunable parameters (current values)

| Parameter | File | Value | Effect |
|---|---|---|---|
| `score_threshold` | `cs9711.c` | **40** (default) | Min consistent angle pairs. No override in the driver — uses the SIGFM default. |
| `distance_match` | `sigfm.cpp` | **0.75** | Ratio test threshold. Higher = accepts more borderline keypoints. |
| `length_match` | `sigfm.cpp` | 0.05 | Geometric length tolerance. |
| `angle_match` | `sigfm.cpp` | 0.05 | Geometric angle tolerance. |
| `min_match` | `sigfm.cpp` | 5 | Min keypoints required before scoring. |

The driver runs with the **default SIGFM parameters** (no custom `score_threshold`
override in `cs9711.c`, `distance_match = 0.75`). Enrollment stores a **single template**
per employee (`<employee_id>.fpdata`); there is no multi-template mode.

---

## Enrollment Flow (Current, Non-Blocking)

### Preferred command flow
1. `start_fingerprint_enroll(employeeId, finger)`  
   Creates background job and returns `job_id`.
2. `poll_fingerprint_enroll(jobId, fromIndex)`  
   Returns incremental lines and job state (`running`, `done`, `failed`).
3. UI loops polling every ~250ms and updates log box in real time.

This model avoids relying on a single long blocking `invoke()` for live progress.

### Shared in-memory state
`AppState` tracks:
- `enroll_jobs` (job map for background enrollment)
- `enroll_job_seq` (job ID counter)
- `auth_jobs` (job map for background identification)
- `auth_job_seq` (job ID counter)

### Auth (polled, same pattern as enrollment)
1. `start_fingerprint_auth()` → returns `job_id`
2. `poll_fingerprint_auth(jobId, fromIndex)` → incremental lines + state
3. UI shows live scan-quality feedback (attempt N/3, retry reasons) via `ATTEMPT|` and `RETRY|` lines
4. On success resolves `{ employee, source: "fingerprint" }` — same shape as password auth

---

## Helper Spawn Behavior (Rust)

Implemented in `fingerprint.rs`:
- Resolves helper from:
  1. `HPS_FINGERPRINT_HELPER`
  2. extracted bundle in app data dir
  3. build paths under `libfprint-CS9711/...`
- Sets `LD_LIBRARY_PATH` so helper finds bundled `libfprint-2.so.2`
- Runs helper with `current_dir = source_root` (Python-style `BASE_DIR` equivalent)
- Applies timeout via `HPS_FINGERPRINT_TIMEOUT` (default 360s)
- Reads stdout/stderr lines, filters to known protocol prefixes

---

## Payroll & Proration

Implemented in `src-tauri/src/commands/payroll.rs` (pure, DB-free math in
`compute_payroll`, fully unit-tested).

- **Base pay** is **`$28.50/hour`** — it prorates with hours worked. A full
  40-hr week earns `40 × $28.50 = $1140`; a 24-hr week earns `$684`.
- **Proration rule**: a worker makes **36 units/day = 4.5 units/hour** (the
  "magic number"). To earn the base rate you must average that many units per
  hour. Both base pay and the base-unit threshold scale linearly with hours:
  - `base_pay = $28.50 × hours_worked`
  - `threshold = 4.5 × hours_worked`
  A full 40-hr week → base `$1140`, threshold `180`. A 4-day (32-hr) week →
  base `$912`, threshold `144`. Overtime scales both up the same way.
- **Extra units**: units are tallied for the whole week. The first
  `threshold` units are covered by base pay; each unit above earns **$3.80**.
  `gross = base_pay + extra_units × $3.80`.
- **Review band**: a week whose clocked hours fall outside **39–41** has an
  unusual proration and is flagged `status = "review"` + a yellow
  `payroll_proration` alert (deduped per employee+week) so an admin can verify
  the hours.
- **Admin actions** (Payroll panel, only shown for `review` weeks):
  - *Accept Prorated* → keep the clocked-hours proration (base pay + threshold),
    mark the week reviewed.
  - *Use Standard 180* → treat the week as a standard **40-hr** week (base
    `$1140`, threshold `180`) regardless of clocked hours, mark it reviewed.
  - Both persist via `payroll_periods.use_standard_week` (0 = prorate by clocked
    hours, 1 = force standard week) and `payroll_periods.reviewed` (1 = admin
    approved), so the decision sticks across recomputes.
- **Unknown rates** (custom/missing unit value) always make a week `unresolved`
  (red alert) and take precedence over the review band; resolved via
  `resolve_unknown_rate`.
- Payroll rows are keyed by `(employee_id, week_start)`; the week is
  **Wednesday-based** (`week_start_for` / `weekStartIso`).

---

## Python Legacy Reference (Use as Behavioral Baseline)

Key files:
- `Hopkins-Inventory-Management-main/admin_enroll_employee.py`
- `Hopkins-Inventory-Management-main/Utilities/fingerprint_service.py`

Use these when validating expected UX/progress wording and subprocess behavior.

---

## Environment Variables

| Variable | Purpose | Default |
|---|---|---|
| `HPS_FINGERPRINT_HELPER` | Override helper binary path | auto-detected |
| `HPS_FINGERPRINT_TIMEOUT` | Helper timeout seconds | `360` |
| `HPS_BACKUP_FORCE` | Dev hook: force `weekly` / `monthly` / `both` backup on next scheduler tick | unset |

---

## Build / Verify

```bash
cd src-tauri
cargo check
cargo test          # 34 Rust tests (db, backup, payroll math + proration, search, storage)
cargo build --release
```

Frontend pure-helper tests (Node's built-in runner, no deps):
```bash
node --test ui/js/test/pure.test.mjs
```

Direct helper smoke test:
```bash
# Enroll (single template)
LD_LIBRARY_PATH=libfprint-CS9711/build/libfprint \
  libfprint-CS9711/build/examples/employee-clock-helper enroll /tmp/fp-test EMP001 right-index

# Identify
LD_LIBRARY_PATH=libfprint-CS9711/build/libfprint \
  libfprint-CS9711/build/examples/employee-clock-helper identify /tmp/fp-test
```

Rebuild helper after C changes:
```bash
cd libfprint-CS9711/build
ninja -j$(nproc)
```

---

## Troubleshooting Priorities

1. **Confirm helper works in terminal first** (must reach `READY|enroll` and progress lines).
2. **Confirm USB permissions** (`sudo` or proper udev rule).
3. **Confirm compiled app is freshly rebuilt** after backend + UI changes.
4. **If UI still stalls, inspect poll responses** from `poll_fingerprint_enroll` (state and line increments).

## graphify

This project has **two separate knowledge graphs**, each with god nodes, community structure, and cross-file relationships:

| Graph | Location | Covers |
|---|---|---|
| app | `graphify-out/` (repo root) | The app: Rust/Tauri backend + UI + docs |
| cs9711 | `libfprint-CS9711/graphify-out/` | The vendored C library: core, all drivers, SIGFM matcher, examples (incl. `employee-clock-helper.c`) |

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Questions about the fingerprint driver internals (cs9711.c, sigfm.cpp, FpiSsm, USB transfer, other drivers) go to the cs9711 graph: `graphify query "<question>" --graph libfprint-CS9711/graphify-out/graph.json` (works from the repo root).
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- **Update each graph from its own scan root — never update the cs9711 graph from the repo root:**
  - App graph: `graphify update .` from the repo root (AST-only, no API cost).
  - CS9711 graph: `cd libfprint-CS9711 && graphify update .` (or `/graphify .` with working directory `libfprint-CS9711/`). The root `.graphifyignore` excludes `libfprint-CS9711/`, so a root scan can never see or refresh it.
- The root `.graphifyignore` excludes the vendored `libfprint-CS9711/` C library from the app graph. Do not remove that exclusion — the driver has its own graph instead.
- The nested `libfprint-CS9711/.graphifyignore` contains `!libfprint/` and `!examples/` negations on purpose: graphify loads ancestor `.graphifyignore` files up to the nearest VCS root, so without them the root's `libfprint-CS9711/` rule collapses a nested scan to root-level files only. Keep the negations above the exclusion lines (last-match-wins).
