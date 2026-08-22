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
  - Enrollment writes `<employee_id>.fpdata` (single template), persists to SQLite, removes temp files.
  - Identify exports templates from SQLite to `data/fingerprints/`, then clears cache after scan.
- `fingerprint_templates` table: one row per `employee_id` (single template). Columns: `employee_id` (PK), `finger` (the enrolled finger), `template` (BLOB), `updated_at`.

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

---

## Build / Verify

```bash
cd src-tauri
cargo check
cargo build --release
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

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- `.graphifyignore` excludes the vendored `libfprint-CS9711/` C library from the graph — the graph covers the app (Rust + UI + docs) only. Do not remove that exclusion; if the helper source (`libfprint-CS9711/examples/employee-clock-helper.c`) needs graph coverage, it is documented here in the Fingerprint sections instead.
