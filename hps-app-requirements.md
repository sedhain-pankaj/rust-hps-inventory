# HPS Kiosk App — Requirements Spec

## 1. Overview
A full-screen, unskippable kiosk app (Rust + Tauri + SQLx, single SQLite database `hps.db`) for staff clock-in/out, cornice production logging, payroll calculation, and inventory tracking. Fingerprint auth (WA28 scanner, CS9711 chipset — already implemented) is the primary login method, with a password fallback after repeated failures.

**Out of scope for now:** Brochure section (build last).

---

## 2. App Structure

- **Start screen:** Shows current date/time, single "Start" button.
- **Main menu:** Admin | Staff | Brochure (disabled/greyed out until built) | Back.
- **Navigation:** Every page has a back button, top-left corner.
- **Deployment:** Single physical kiosk. Admin can either use the kiosk directly or remote into the machine to do admin work — no multi-writer/multi-kiosk sync needed.

---

## 3. Authentication

- **Admin:** One admin account only, pre-seeded in the DB as `EMP001`. Fingerprint required immediately on entering Admin mode.
- **Staff:** Staff mode opens to a **list of employees**. Tapping a name prompts a fingerprint scan:
  - Match → opens that employee's page.
  - No match → access denied, stays on list.
- **Fallback:** After 5 failed fingerprint attempts (any gate — admin or staff), a password field appears as backup. Password is set/changed by admin only, stored hashed.
- **Clock in/out:** Also fingerprint-gated, same fallback rule applies.

---

## 4. Staff Categories

### 4a. Cornice Hands
- Clock in/out via fingerprint.
- Log daily cornice production with **fuzzy search** against the cornice rate table (seeded from `cornice_rate.csv`, now fully migrated into `hps.db` — see §6).
- Unknown cornice (no matching rate): marked `Unknown`, sent as an **alert to admin** (see §5.3 for pay treatment).
- Daily view: tabulated report of cornice types + total units, cumulative through the week.
- Pay week: **Wednesday–Tuesday**. Gross pay + breakdown shown on Tuesday (week close).
- Full payroll rules: §5.

### 4b. Store-keeper
- Clock in/out.
- Full read access to the cornice production log.
- Manages **cornice stock** — which cornice castings, which aisle, quantity in stock, quantity reserved for orders, free-text remarks. (Distinct from mould inventory — see §6.)
- Manages **mould inventory** — add/edit entries (mould storage location).
- Views production log, dispatch log.

### 4c. Non-cornice / Custom Order Staff
- Clock in/out.
- Logs what was made via an "add" button: order name + quantity. No rates involved (no piece-rate pay calc for this role).

### 4d. Driver
- Clock in/out.
- Views dispatch orders admin has created (cornice type, quantity, delivery location).
- Logs actual delivery: what was delivered, quantity, location, remarks.

### 4e. Helpers
- Clock in/out.
- Read-only view of cornice mould/stock locations.

### 4f. All staff (shared)
- Read-only view of mould inventory table (name of cornice mould, location). Editable only by store-keeper or admin.

---

## 5. Payroll Logic — Cornice Hands

### 5.1 Base rules
- Base pay: 8 hrs × 5 days × $28.50/hr = **$1,140/week**, assuming a standard ~40-hour week.
- Base unit requirement: 36 units/day → **180 units/week**.
- Units above 180/week are **extra**, paid at **$3.80/unit**.
- Worked example (standard week): 500 units → 500 − 180 = 320 extra → 320 × $3.80 = $1,216 → total $2,356.

### 5.2 Underperformance
- If cumulative weekly units < 180, the system **floors at 180** for pay purposes — i.e. base pay ($1,140) is still paid in full, no deduction below it, and no extra-unit pay (since there's no surplus). No penalty beyond not earning the extra-unit bonus.

### 5.3 Unknown-rate cornices
- Unknown cornice units are **not** converted to a placeholder dollar amount automatically. Instead, the day's/week's pay report shows an **equation**, e.g.:
  `400 units + 10×FL1 + 20×FL2 − 180 (base units)`
  where `400` is the known-rate unit total for the period and `FL1`/`FL2` are unresolved cornice names with their logged quantities.
- A **red alert** is raised in the admin panel flagging unresolved rates.
- Once admin assigns a rate to the unknown cornice, the equation **resolves automatically** and the affected day/week's pay recalculates to a concrete number.
- *(Open question: do you want a hard block on generating a final payslip PDF while unresolved units exist for that pay period, or should it export with the equation left in symbolic form? Worth deciding before you build the PDF export.)*

### 5.4 Attendance-adjusted base (39–41 hr band)
This is the most detail-sensitive rule, so stating it precisely:

- **Normal band — 39.00 to 41.00 hrs/week:** Treat as a standard 40-hour week. Use the flat 180-unit threshold from §5.1/§5.2. No alert, no proration.
- **Outside the band — <39 hrs or >41 hrs:** The system computes a **prorated unit threshold** using 4.5 units/hour (36 units ÷ 8 hrs = 4.5), i.e. `threshold = actual_hours × 4.5`, instead of the flat 180.
  - Example given: 39 hrs actual, 400 units made → threshold = 39 × 4.5 = 175.5 → extra units = 400 − 175.5 = 224.5 → extra pay = 224.5 × $3.80.
  - This condition **raises an alert to admin** rather than applying silently. The alert should let admin choose:
    - (a) Accept the prorated threshold calculated above, or
    - (b) Override and treat it as a standard 40-hr / 180-unit week regardless of actual hours.
  - Rationale: rigid auto-proration could trigger disputes with staff; admin gets discretion.
- **Base pay itself in the outside-band case:** the spec doesn't yet say whether the $1,140 base is also prorated by actual hours, or stays flat and only the *unit threshold* is prorated. Recommend: keep base pay flat at $1,140 unless admin explicitly adjusts it manually (via the clock in/out edit function in §5.5) — only the extra-unit threshold auto-prorates. Confirm this matches your intent.

### 5.5 Clock-in/out correction
- If a staff member misses a clock-in or clock-out, they (or store staff) can flag it, and **admin can manually edit** the recorded time.
- Recommend: every admin edit to a clock event is logged with a timestamp, the admin's ID, old value, new value, and an optional reason note — so there's an audit trail on payroll-affecting edits. (Not stated in your requirements, but worth having given this directly affects pay.)

---

## 6. Data Model Notes

- **`mould_inventory`** — mould storage: which mould, which storage area. Read-only for all staff, editable by store-keeper/admin.
- **`cornice_stock`** — actual castings: which cornice, which aisle, quantity in stock, quantity reserved, remarks. Editable by store-keeper.
- **`cornice_rate`** — rate table, originally seeded from `cornice_rate.csv` (legacy Python project format), now fully migrated into `hps.db` as the live source of truth. All staff can **view**; only admin can **modify** rates (already implemented per your note).
- **Portability:** single kiosk, single SQLite file — no multi-writer concerns given the one-kiosk-plus-remote-admin model.

---

## 7. Kiosk Lockdown

Tauri handles the full-screen webview, but true "unskippable kiosk" behavior (blocking Alt+Tab, VT switching, desktop shell access, etc.) is an OS-level configuration on Kubuntu — autologin into a locked-down session, disabling window manager shortcuts, etc. This sits alongside the Tauri build as a separate systems task, not something the app can enforce from inside the webview alone.

---

## 8. Open Items / Suggested Additions

1. **PDF export scope during unresolved rates** (§5.3) — block, or export with symbolic equation.
2. **Base pay proration in the outside-band case** (§5.4) — confirm base stays flat unless admin manually adjusts.
3. **Audit log for clock-in/out edits** — suggested addition, not yet in your spec.
4. **Admin alert center** — you've mentioned alerts for unknown rates and attendance-band exceptions; worth designing this as one unified "Alerts" inbox in the admin panel rather than separate ad-hoc notices, since both feed the same review-and-resolve workflow.
5. **Item 15 in your original doc was blank** — flagging again in case something was meant to go there.
6. **Brochure button state** — still needs a decision: hidden entirely vs. visibly disabled until built.

---

*Fingerprint hardware integration (WA28/CS9711) already completed — no longer an open risk.*
