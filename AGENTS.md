# Hopkins Inventory Management - Agent Reference (Current)

## Project Snapshot

Hopkins Plaster Studio kiosk + inventory system has two codebases:

| Version | Path | Role |
|---|---|---|
| Rust/Tauri (active) | `src-tauri/`, `ui/` | Production app in active development |
| Python legacy (reference) | `Hopkins-Inventory-Management-main/` | Historical reference for behavior and helper flow |

The fingerprint stack is shared via the C helper binary:
`libfprint-CS9711/build/examples/employee-clock-helper`.

---

## Current Runtime Architecture

### Backend (Rust/Tauri)
- `src-tauri/src/commands.rs`: Tauri commands and enrollment orchestration
- `src-tauri/src/fingerprint.rs`: helper discovery, spawn, protocol parsing, template import/export
- `src-tauri/src/db.rs`: app paths, SQLite init, shared app state
- `src-tauri/src/models.rs`: command response/request models

### Frontend (Web UI)
- `ui/js/app.js`: admin/staff screens, enrollment polling UI
- `ui/js/api.js`: Tauri invoke wrappers

### Persistence
- SQLite DB: `hps.db` (repo root)
- Fingerprint temp/cache files: `data/fingerprints/`
  - Enrollment writes `<employee_id>.fpdata`, then persists to SQLite, then removes temp file.
  - Identify exports templates from SQLite to `data/fingerprints/`, then clears cache after scan.

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

Only these protocol lines are consumed by Rust from helper output.

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
| `HPS_FINGERPRINT_ATTEMPTS` | Identify retry attempts | `5` |

---

## Build / Verify

```bash
cd src-tauri
cargo check
cargo build --release
```

Direct helper smoke test:
```bash
LD_LIBRARY_PATH=libfprint-CS9711/build/libfprint \
  libfprint-CS9711/build/examples/employee-clock-helper enroll /tmp/fp-test EMP001 right-index
```

---

## Troubleshooting Priorities

1. **Confirm helper works in terminal first** (must reach `READY|enroll` and progress lines).
2. **Confirm USB permissions** (`sudo` or proper udev rule).
3. **Confirm compiled app is freshly rebuilt** after backend + UI changes.
4. **If UI still stalls, inspect poll responses** from `poll_fingerprint_enroll` (state and line increments).
