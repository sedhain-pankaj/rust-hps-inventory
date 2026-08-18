# HPS Kiosk Fixes — Design Spec

Date: 2026-08-18
Status: Approved by user
Scope: UI polish (timer, fingerprint modal), overall UX (partial re-render, stock merge, mould locations, rates, icons, storage info), staff workflow (entry edit/delete with approval, tabular logs, clock in/out modals), and a shared inline-table editing component applied to a core batch of panels.

## Context

Two codebases: Rust/Tauri backend (`src-tauri/src/`) and web UI (`ui/`). All screens render into `#app`; modals render into `#modal-root`. The backend has no server-side permission checks on data commands (client-side gating only) — this spec does not change that.

Key existing code:
- `ui/js/app.js` (2017 lines): all screens. `renderAdmin()` app.js:273, `renderStaffDashboard()` app.js:1252, `setPanel()` app.js:1795, `workspaceShell()` app.js:1750, `tabIcons` app.js:1728, home clock `renderHome()` app.js:76, session timer `startSessionTimer()` app.js:141.
- `ui/js/auth.js`: `requestAuth()` auth.js:39, `chooseClockAction()` auth.js:239, `closeModal()` auth.js:34.
- `ui/js/icons.js`: inline SVG icon set, `icon(name, size)` renderer.
- `ui/styles.css`: design system (1065 lines).
- `src-tauri/src/commands.rs` (3211 lines): Tauri commands. `src-tauri/src/db.rs` (1152 lines): schema + migrations (`migrate()`, `_schema_migration_log` tracking).

---

## 1. TIMER (home clock)

1. **Seconds color**: remove the `.clock-time .secs { color: var(--brass); }` rule (styles.css:266-268). All digits render in the uniform white/beige color of `.clock-time`.
2. **Logo size**: `.clock-logo` (styles.css:249-255) width 64px → 100px.
3. **Date size**: `.clock-date` (styles.css:270-277) font-size `clamp(1.4rem, 3vw, 2.6rem)` → `clamp(1.7rem, 3.5vw, 3.2rem)`.
4. **<10s red pulse**: already implemented — `app.js:155-160` adds `.danger` at ≤10s and `.warn` at ≤60s; `.status-pill.danger` (styles.css:384-389) has the red `pulse` animation. No code change; verify visually during testing.

## 2. FINGERPRINT (auth scan modal)

1. **Off-center glyph**: `ui/assets/noun-fingerprint-1377758.svg` has `viewBox="0 0 100 125"` but all drawing occupies `0 0 100 100`, leaving 25 units of dead space below the glyph, so it renders above the ring's center. Fix: change the viewBox to `0 0 100 100` (also fix the `style="enable-background:new 0 0 100 100"` consistency). No CSS change needed — `.auth-fp-icon` is already a centered 128×128 grid (styles.css:920-928).
2. **Yellowish bracket bar**: `.scan-status` (styles.css:877-888) has `border-left-width: 4px` which reads as a square bracket at the start of the status box. Remove the left-border treatment entirely (all variants: info/warn/ok/err keep their tinted background + text color, which remains distinguishable).

## 3. OVERALL

### 3.1 Partial re-render on tab click

Currently every tab click calls `renderAdmin()` / `renderStaffDashboard()`, which rebuilds the entire `#app` (topbar + side nav + empty panel) and then fills the panel.

Fix: build the workspace shell once. On tab click:
1. Update `state.adminView` / `state.staffView`.
2. Toggle `.active` class on the existing `[data-tab]` buttons (no innerHTML rebuild).
3. Call `renderAdminPanel()` / `renderStaffPanel()` to fill only the right pane via `setPanel()`.

Applies to both admin and staff dashboards. Panel content, forms, and async data loading are otherwise unchanged.

### 3.2 Merge CORNICE STOCK into STOCKS

**Migration** (in `db.rs::migrate()`, idempotent, recorded in `_schema_migration_log`):
- `ALTER TABLE stock_items ADD COLUMN reserved INTEGER NOT NULL DEFAULT 0` (guarded, following the existing `staff_category` migration pattern at db.rs:530-579).
- Copy rows (only where no `stock_items` row with the same `(item_type='cornice', model)` exists): `INSERT INTO stock_items (item_type, model, stock, location, reserved, notes) SELECT 'cornice', model, quantity_in_stock, aisle, quantity_reserved, remarks FROM cornice_stock`.
- The `cornice_stock` table is kept (no data drop); its dedicated UI tab and commands are removed from the UI. Backend commands `list_cornice_stock` / `save_cornice_stock` are removed from the command registration and deleted from `commands.rs`/`models.rs` (no callers remain).

**UI — single Stocks tab** (admin tab id `stock`, label "Stocks"):
- Item-type filter select: All / Cornice / Other ("Other" = `item_type != 'cornice'`).
- Columns: Type, Model, Location, Stock, Reserved, Notes.
- Type cell/edit offers two values: `cornice` and `other` (stored verbatim in `item_type`).
- Uses the new inline-table component (§5).
- Storekeeper staff read-only tab becomes "Stocks (Read-Only)" over `stock_items` (all types, with Type column).

### 3.3 MOULD LOCATIONS (extensible)

**Schema**: new table `mould_locations (id INTEGER PK, name TEXT UNIQUE NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0)`, seeded with:
1. Near Dryer
2. Singles Wall
3. Doubles Wall

`mould_inventory.storage_location` continues to store the location name (existing rows keep working).

**Commands**: `list_mould_locations`, `save_mould_location` (insert/rename), `delete_mould_location` (only when no moulds reference it). `save_mould_inventory` unchanged (location is a free string validated client-side against the location list).

**UI — "Mould Locations" tab** (admin tab id `mould_inventory`, relabeled):
- One box per location, ordered by `sort_order`: header shows location name + mould count.
- Each box: inline table (Mould Name / Notes) + **Add row** (via the shared inline-table component, §5).
- **Add location** button (panel actions) → adds a new location (name prompt via small inline form/modal).
- Delete a location only if it has zero moulds.
- Mould location field is a select of existing locations (no free text).
- Legacy rows whose `storage_location` matches no `mould_locations` entry (including empty) render in a trailing **Unassigned** section (admin can reassign them via the location select; staff see it read-only). Never hidden.
- Staff read-only mould view: same per-location boxes, no add/edit/delete.

### 3.4 RATES — grouped by series

Admin rates panel (and staff read-only rates view):
- Group rows by `series` (sorted). Each series renders as a section header; under it a compact table: Model / Unit / Value (admin) or Model / Unit (staff — value hidden), value column right-aligned with `font-variant-numeric: tabular-nums`.
- Edit form stays on top (Series, Model, Unit Text, Unit Value, Confidential).
- Admin uses the inline-table component; editing a Series cell re-groups on save.

### 3.5 New icons

Add to `ui/js/icons.js` (Lucide/Feather-style 24×24 stroke paths):
- `package` — boxed crate, for **Stocks**.
- `calculator` — for **Rates**.
- `map-pin` — for **Mould Locations**.

Update `tabIcons` (app.js:1728-1748):
- `stock` / `cornice_stock_ro` → `package`
- `rates` / staff `rates` → `calculator`
- `mould_inventory` / staff `moulds` → `map-pin`
- Remove the now-dead `cornice_stock: "box"` entry.

### 3.6 Database section — storage info + disk alert

**Backend**: new command `storage_status` returning:
```
{ db_path, db_size_bytes, disk_total_bytes, disk_free_bytes, disk_used_pct }
```
- DB size: `std::fs::metadata` on the DB path.
- Disk: `libc::statvfs` on the directory containing the DB file (libc is already a dependency — no new crate). `disk_used_pct = 100 × (1 - f_bavail / f_blocks)`.

**UI**: database panel header shows e.g. `DB 12.4 MB · Disk 63% used · 212 GB free`.

**Alert**: when `disk_used_pct > 90`, insert a red `admin_notifications` row (kind `disk_space`) — deduped: only if no unresolved `disk_space` alert already exists. Checked at app boot (UI init calls `storage_status` once) and whenever the database panel opens.

## 4. STAFF

### 4.1 Edit/delete own cornice log entries

**Schema** (migration, idempotent):
- `cornice_logs`: add `prev_values TEXT` (JSON of previous values of changed fields, NULL when none), `amended_at TEXT`, `amended_by TEXT`.

**Commands** (new):
- `update_cornice_log(request { id, actor_id, series, model, lengths })`:
  - Entry must belong to `actor_id` (otherwise error).
  - If `log_date == today`: apply immediately; set `amended_at`/`amended_by`; `needs_admin_review` unchanged (0).
  - If `log_date < today`: apply values, store previous values of changed fields in `prev_values` (JSON), set `needs_admin_review = 1`, set `amended_at`/`amended_by`, insert a yellow `admin_notifications` row (kind `cornice_log_edit`, entity `cornice_logs`/id): "«name» edited a past entry (date, model) — pending approval."
  - Re-look up the rate when model changes (same logic as `add_cornice_log`, commands.rs:1382-1391) and recompute `total_units`; if the new model has no rate, `is_custom = 1` and review is required regardless of date.
- `delete_cornice_log(id, actor_id)`: same ownership + same-day/older-day rules. Older-day delete: instead of deleting, mark the row with `prev_values = {"deleted": true, ...old field values}`, `needs_admin_review = 1`, set amended fields, yellow alert. On approval the row is actually deleted. Same-day delete: immediate hard delete.
- `approve_cornice_log(id)` (admin): if the row is a staged delete → hard delete; else clear `prev_values` and `needs_admin_review = 0`; resolve the linked unresolved `cornice_log_edit` alert.

**Rendering (staff view + admin Daily Logs panel)**:
- Any amended entry shows a **✎** cue next to the model.
- Pending entries (older-day edit/delete) render changed cells as **`<s>old</s> → new`** (old value struck through, arrow, new value) — e.g. `<s>8</s> → 7` for lengths/units; staged deletes render the whole row struck through with a "pending deletion" tag.
- After approval: cell shows only the new value (✎ remains).
- Admin **Daily Logs** panel: pending rows additionally get an **Approve** button.

### 4.2 Tabular staff cornice log

Staff cornice panel layout:
- Add-entry capability via the inline-table component (see §5): "+ Add" appends an editable row; the Model cell uses the existing datalist + `search_cornice_rates` fuzzy hint and auto-fills Unit (existing behavior app.js:1401-1436).
- Below: **week boxes** (outer container per week using the existing `weekStartIso()` convention, header = week range + week total units) containing **day boxes** (header = date + day total units), each day a compact table: Model (with ✎ / pending rendering) / Lengths / Unit / Units.
- Order: most recent week first, most recent day first within a week, entries descending.
- Row cells are editable (Model, Lengths) and deletable per §4.1 rules; dirty rows highlight; Save icon commits.

### 4.3 Clock in/out — no re-auth, smart button, confirm modals

**No re-auth**: the staff dashboard already knows the logged-in employee (`state.currentStaff`). Remove the `requestAuth()` call in `renderStaffClock()` (app.js:1333-1355). Event `source` reuses the login source: store it in `state.sessionSource` at login time (fingerprint/password) and pass it to `record_clock_event`.

**Backend**: new command `get_clock_status(employee_id)` returning:
```
{ today_state: "none" | "in" | "out", missed_yesterday_clock_out: bool }
```
- `today_state` from today's `time_clock_events` (last action).
- `missed_yesterday_clock_out`: yesterday has a `clock_in` with no later `clock_out` (same logic as `refresh_attendance_issues`, commands.rs:3003-3060, but read-only).

**UI flow** (`renderStaffClock`):
- Button label from `today_state`: `none` → "Clock In"; `in` → "Clock Out"; `out` → "Clock In" (re-entry).
- Click → new generic `confirmModal()` (added to `auth.js`, replaces `chooseClockAction`):
  - Clock in: "Clock in now?" — if `missed_yesterday_clock_out`, include a warning line: "You didn't clock out yesterday." (The existing red `missing_clock_out` admin alert fires automatically inside `record_clock_event` via `refresh_attendance_issues` — no new backend work.)
  - Clock out: "Clock out now?"
- On confirm → `record_clock_event` with `state.currentStaff.id`, chosen action, `state.sessionSource`.
- Same-day-only is already enforced server-side (`work_date = today`, commands.rs:1048); the UI never offers other days.

## 5. INLINE TABLE COMPONENT (core batch)

New shared component, e.g. `ui/js/table.js`, exporting a helper that renders and manages an editable table.

**Configuration**:
- `columns`: `[{ key, label, type: "text" | "number" | "select" | "bool", options?, editable?, width? }]`
- `rows`: data rows (with id)
- `canEdit(row)`, `canDelete(row)` predicates
- `commit`: `{ save(row) → Promise, remove(id) → Promise, add(row) → Promise }` mapping to existing Tauri commands
- Optional `groupBy` (used by Rates: group by series)

**Behavior**:
- **Click a cell** (if editable and row allowed) → cell becomes an input / select / number field; on change the row is marked dirty (subtle background highlight). Clicking away or Esc reverts that cell.
- **Add**: "+ Add" button in panel actions appends an empty draft row (all editable cells active).
- **Delete**: per-row ✕ (when `canDelete`) stages the deletion — row dims / strikes through; committed on save.
- **Save icon** in panel actions (position where Refresh was): enabled only when there are pending changes (dirty rows, staged deletes, draft adds). Click commits everything: `add()` for drafts, `save()` for dirty rows, `remove()` for staged deletes (sequentially; on first error stop, keep all drafts, surface the error). On success clear drafts and re-render from DB.
- **Discard** (small × next to Save): reverts all drafts to last DB state.
- When there are no pending changes, the Refresh button shows instead of the Save icon.
- Read-only mode (staff views): no cell editing, no add/delete/save — plain table.

**Panels converted in this iteration (core batch)**:
1. **Stocks** (merged, §3.2) — admin editable; storekeeper read-only.
2. **Mould Locations** (§3.3) — admin editable per location box; staff read-only.
3. **Rates** (§3.4) — admin editable with series grouping; staff read-only grouped.
4. **Staff cornice log** (§4.2) — self-entries editable per §4.1 rules.

**Not converted now** (keep existing click-row→form pattern; component is designed so they can be converted later): Employees, Dispatch, Overstock, Deliveries, Production logs, the generic Database table editor.

## 6. Known issue — OUT OF SCOPE (remind user at end of task)

**Fingerprint cross-match**: enrolling a new employee's fingerprint works, but then previously enrolled fingers stop matching their own employee; identify returns the *other* employee ("this session is not for you, but for 1st staff"). Sensor capture is confirmed accurate (different physical fingers). Lowering `score_threshold` 40→24 and `distance_match` 0.80→0.75 did not help. Relevant git history: `174e7eb` (3 fingerprint scans / multi-template), `ea6d277` (fixed polling similarity), `8951f81` (fixed auto-retry error), `63a73e3` (kills old helper on new spawn). To investigate after this task completes.

## Verification

- `cd src-tauri && cargo check && cargo build --release`
- UI smoke: home clock (uniform color, logo/date sizes), session pill pulses red <10s, auth modal (centered glyph, no bracket bar), tab clicks don't rebuild the shell (verify via DevTools that side nav DOM nodes persist), Stocks filter + inline add/edit/delete/save/discard, Mould Locations add-location + per-section rows, Rates grouping + series cell edit, Database storage line + disk alert at >90% (simulatable by lowering threshold in a test), staff log week/day boxes + `<s>old</s> → new` rendering + approve flow, clock in/out confirm modals + skipped re-auth + missed-clock-out warning.
- Migration idempotency: run app twice on an existing `hps.db`; verify no duplicate stock rows and no errors.
