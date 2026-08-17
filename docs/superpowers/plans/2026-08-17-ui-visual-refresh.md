# UI Visual Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the Hopkins kiosk UI into a "polished plaster studio" look (warm charcoal, cream, brass) with zero changes to application logic.

**Architecture:** Design tokens + restyle in place. Task 1 adds assets (bundled font, SVG icon module). Task 2 rewrites `ui/styles.css` as the full design system (it styles both the current markup and the new markup introduced by later tasks, so every intermediate commit looks intentional). Tasks 3–5 migrate the JS template strings to the new markup/classes. Task 6 updates the Tauri window background and runs the final sweep.

**Tech Stack:** Vanilla HTML/CSS/JS (no build step, no framework), Tauri v2 webview, SQLite backend (untouched). No test framework exists in this repo — verification is python3 scripts, grep sweeps, `cargo check`, and user visual checks.

**Spec:** `docs/superpowers/specs/2026-08-17-ui-visual-refresh-design.md`

## Global Constraints

- No changes to Rust source (`src-tauri/src/**`). Only `src-tauri/tauri.conf.json` `backgroundColor` changes (Task 6).
- No changes to Tauri command names, invoke() call signatures, event bindings, or render-function logic. Template strings and class names only.
- All `data-*` attributes referenced by JS must be preserved: `data-start`, `data-back`, `data-role`, `data-employee`, `data-tab`, `data-panel-title`, `data-panel-actions`, `data-panel-body`, `data-status-pill`, `data-session-remaining`, `data-date`, `data-enrollment-log`, `data-refresh`, `data-close`, `data-password`, `data-password-submit`, `data-action`, and all `data-*` row/button attributes.
- No new dependencies, no CDN/external URLs (kiosk is offline). Font and icons are local files.
- Kiosk touch targets unchanged: buttons `min-height: 52px`, side-nav items `min-height: 58px`.
- All motion must be disabled under `prefers-reduced-motion: reduce`.
- Verification tooling available: `python3` (3.14), `cargo`, `grep`. NO node, NO headless browser. Visual checks are performed by the user opening `ui/index.html` in a browser or running the app.
- Commit style: lowercase, terse (match existing history, e.g. "fixed polling similarity").
- Work from repo root: `/home/sedhain_pankaj/Desktop/rust-hps-inventory`.

---

### Task 1: Bundle Inter font + create icon module

**Files:**
- Create: `ui/assets/fonts/inter-var.woff2` (downloaded binary)
- Create: `ui/js/icons.js`

**Interfaces:**
- Consumes: nothing
- Produces: `icon(name, size?)` → SVG string; `ICON_NAMES` → array of all 20 icon names. Icon names: `back-arrow, fingerprint, clock, user, users, shield, book, box, layers, truck, gauge, database, bell, alert-triangle, dollar, list, check, x, plus, refresh`. Tasks 3–5 call `icon(name)` inside template literals.

- [ ] **Step 1: Download the variable font**

```bash
mkdir -p ui/assets/fonts
curl -sL --max-time 60 -o ui/assets/fonts/inter-var.woff2 "https://rsms.me/inter/font-files/InterVariable.woff2"
```

- [ ] **Step 2: Verify the font file**

Run: `file ui/assets/fonts/inter-var.woff2 && ls -la ui/assets/fonts/inter-var.woff2`
Expected: `Web Open Font Format (Version 2)...` and size between 300000 and 400000 bytes (~352KB). If the file is HTML or missing, re-run Step 1.

- [ ] **Step 3: Create `ui/js/icons.js` with exactly this content**

```js
const ICONS = {
  "back-arrow": '<path d="M19 12H5"/><path d="M12 19l-7-7 7-7"/>',
  fingerprint: '<path d="M12 10a2 2 0 0 0-2 2c0 1.02-.07 2.03-.2 3.03"/><path d="M14 13.12c0 2.38 0 6.38-1 8.88"/><path d="M17.29 21.02c.12-.6.43-2.3.5-3.02"/><path d="M2 12a10 10 0 0 1 18-6"/><path d="M2 16h.01"/><path d="M21.8 16c.2-2 .13-5.35 0-6"/><path d="M5 19.5C5.5 18 6 15 6 12a6 6 0 0 1 .34-2"/><path d="M8.65 22c.21-.66.45-1.32.57-2"/><path d="M9 6.8a6 6 0 0 1 9 5.2v2"/><path d="M14.83 17.09c.63 1.53 1.07 3.19 1.26 4.91"/><path d="M17.69 11a10 10 0 0 1 1.22 4.51"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M5 21c0-3.9 3.1-6 7-6s7 2.1 7 6"/>',
  users: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20c0-3.6 2.9-5.5 6.5-5.5s6.5 1.9 6.5 5.5"/><path d="M16 4.7a3.5 3.5 0 0 1 0 6.6"/><path d="M18.8 15.2c1.7.9 2.7 2.4 2.7 4.3"/>',
  shield: '<path d="M12 3l7 3v5.5c0 4.3-2.9 7.7-7 9.5-4.1-1.8-7-5.2-7-9.5V6z"/>',
  book: '<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>',
  box: '<path d="M21 8.5l-9-5-9 5v7l9 5 9-5z"/><path d="M3 8.5l9 5 9-5"/><path d="M12 13.5V21"/>',
  layers: '<path d="M12 2L2 7l10 5 10-5z"/><path d="M2 12l10 5 10-5"/><path d="M2 17l10 5 10-5"/>',
  truck: '<path d="M2 6h12v10H2z"/><path d="M14 10h4l3 3v3h-7"/><circle cx="6.5" cy="18" r="2"/><circle cx="17.5" cy="18" r="2"/>',
  gauge: '<path d="M4.5 18.5a9.5 9.5 0 1 1 15 0"/><path d="M12 14l3.5-5"/>',
  database: '<ellipse cx="12" cy="5.5" rx="8" ry="3"/><path d="M4 5.5v13c0 1.66 3.58 3 8 3s8-1.34 8-3v-13"/><path d="M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3"/>',
  bell: '<path d="M6 9a6 6 0 0 1 12 0c0 5 2 6.5 2 6.5H4S6 14 6 9"/><path d="M10.3 19.5a2 2 0 0 0 3.4 0"/>',
  "alert-triangle": '<path d="M12 3.5L2.5 19.5h19z"/><path d="M12 9.5v4.5"/><path d="M12 17.2v.3"/>',
  dollar: '<path d="M12 2.5v19"/><path d="M16.5 6.8c-1.1-1.3-2.8-2-4.7-2-2.6 0-4.6 1.4-4.6 3.6 0 4.6 9.4 2.4 9.4 7 0 2.3-2.1 3.7-4.8 3.7-2 0-3.8-.7-4.9-2"/>',
  list: '<path d="M8.5 6h12"/><path d="M8.5 12h12"/><path d="M8.5 18h12"/><path d="M4 6h.01"/><path d="M4 12h.01"/><path d="M4 18h.01"/>',
  check: '<path d="M4.5 12.5l5 5L19.5 6.5"/>',
  x: '<path d="M6 6l12 12"/><path d="M18 6L6 18"/>',
  plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
  refresh: '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
};

export function icon(name, size = 24) {
  const paths = ICONS[name] || "";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

export const ICON_NAMES = Object.keys(ICONS);
```

- [ ] **Step 4: Verify icon module structure**

```bash
python3 - <<'EOF'
import re
src = open("ui/js/icons.js").read()
expected = {"back-arrow","fingerprint","clock","user","users","shield","book","box","layers","truck","gauge","database","bell","alert-triangle","dollar","list","check","x","plus","refresh"}
keys = set(re.findall(r'^\s{2}["\']?([\w-]+)["\']?\s*:', src, re.M))
missing = expected - keys
assert not missing, f"missing icons: {missing}"
assert "export function icon(name" in src
assert src.count("<svg") == 1, "icon() must build svg in one place"
print(f"icons.js OK: {len(keys)} icons")
EOF
```
Expected: `icons.js OK: 20 icons`

- [ ] **Step 5: Commit**

```bash
git add ui/assets/fonts/inter-var.woff2 ui/js/icons.js
git commit -m "bundle inter variable font + inline svg icon set"
```

---

### Task 2: Rewrite styles.css as the design system

**Files:**
- Modify: `ui/styles.css` (full file replacement)

**Interfaces:**
- Consumes: `ui/assets/fonts/inter-var.woff2` (Task 1). Styles both the CURRENT markup (so the app looks right immediately) and the NEW markup introduced by Tasks 3–5: `.clock-logo`, `.secs`, `.tile-icon`, `.avatar`, `.table-wrap`, `.tag-ok/.tag-warn/.tag-err`, `.scan-status` + `.info/.warn/.ok/.err`, `.metric-ok/.metric-warn/.metric-err`, `.cancel-text`, `.auth-fp-icon .ring`.
- Produces: all visual styling for the rest of the plan. Class names are the contract — Tasks 3–5 must use exactly these.

- [ ] **Step 1: Replace the entire content of `ui/styles.css` with exactly this**

```css
/* ============================================================
   Hopkins Plaster Studio — design system
   "Polished plaster studio": warm charcoal, cream, brass.
   ============================================================ */

:root {
  color-scheme: dark;

  /* Surfaces */
  --bg-0: #141210;
  --bg-1: #1c1917;
  --bg-2: #262220;
  --bg-3: #322d29;
  --bg-inset: #100e0c;

  /* Text */
  --text-1: #f2ead9;
  --text-2: #b8ad9c;
  --text-3: #7d7466;

  /* Lines */
  --line: #3a342e;
  --line-2: #4a423a;

  /* Accents */
  --brass: #d9a441;
  --brass-2: #e6b85c;
  --brass-ink: #1a1508;
  --brass-tint: rgba(217, 164, 65, 0.12);
  --brass-glow: rgba(217, 164, 65, 0.25);

  --ok: #5aa469;
  --ok-tint: rgba(90, 164, 105, 0.12);
  --warn: #e07b39;
  --warn-tint: rgba(224, 123, 57, 0.14);
  --err: #d95c5c;
  --err-tint: rgba(217, 92, 92, 0.12);

  /* Shape */
  --radius-s: 10px;
  --radius-m: 14px;
  --radius-l: 16px;

  /* Elevation */
  --shadow-1: 0 8px 24px rgba(0, 0, 0, 0.35);
  --shadow-2: 0 24px 80px rgba(0, 0, 0, 0.5);

  font-family: "Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
}

@font-face {
  font-family: "Inter";
  src: url("./assets/fonts/inter-var.woff2") format("woff2");
  font-weight: 100 900;
  font-style: normal;
  font-display: swap;
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  width: 100%;
  height: 100%;
  margin: 0;
}

body {
  overflow: hidden;
  color: var(--text-1);
  user-select: none;
  background-color: var(--bg-0);
  background-image:
    radial-gradient(ellipse 90% 55% at 50% 12%, rgba(217, 164, 65, 0.05), transparent 65%),
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='240' height='240'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2' stitchTiles='stitch'/%3E%3CfeColorMatrix type='saturate' values='0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)' opacity='0.05'/%3E%3C/svg%3E");
}

button,
input,
select,
textarea {
  font: inherit;
}

button {
  min-height: 52px;
  border: 1px solid transparent;
  border-radius: var(--radius-s);
  padding: 0 20px;
  background: var(--bg-2);
  color: var(--text-1);
  cursor: pointer;
  transition:
    background 0.12s ease,
    border-color 0.12s ease,
    color 0.12s ease,
    transform 0.12s ease,
    box-shadow 0.12s ease;
}

button:hover {
  background: var(--bg-3);
}

button:active {
  transform: scale(0.98);
}

button:focus-visible,
input:focus-visible,
select:focus-visible,
textarea:focus-visible {
  outline: none;
  border-color: var(--brass);
  box-shadow: 0 0 0 3px var(--brass-glow);
}

button.primary {
  background: var(--brass);
  color: var(--brass-ink);
  font-weight: 700;
}

button.primary:hover {
  background: var(--brass-2);
}

button.warning {
  background: var(--warn);
  color: var(--brass-ink);
  font-weight: 700;
}

button.warning:hover {
  filter: brightness(1.1);
}

button.danger {
  background: var(--err);
  color: #fff;
  font-weight: 700;
}

button.danger:hover {
  filter: brightness(1.1);
}

button.ghost {
  border-color: var(--line);
  background: transparent;
  color: var(--text-2);
}

button.ghost:hover {
  background: var(--bg-2);
  color: var(--text-1);
  border-color: var(--line-2);
}

button.icon {
  width: auto;
  min-height: 52px;
  padding: 0 14px;
  display: flex;
  align-items: center;
  justify-content: center;
}

button.icon svg {
  width: 22px;
  height: 22px;
}

button.icon img {
  width: 24px;
  height: 24px;
  object-fit: contain;
  filter: invert(1);
}

button:disabled {
  opacity: 0.5;
  cursor: wait;
}

input,
select,
textarea {
  width: 100%;
  border: 1px solid var(--line);
  border-radius: var(--radius-s);
  padding: 13px 14px;
  background: var(--bg-inset);
  color: var(--text-1);
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}

input::placeholder,
textarea::placeholder {
  color: var(--text-3);
}

input:focus,
select:focus,
textarea:focus {
  outline: none;
  border-color: var(--brass);
  box-shadow: 0 0 0 3px var(--brass-glow);
}

textarea {
  min-height: 88px;
  resize: none;
}

label {
  display: grid;
  gap: 7px;
  color: var(--text-2);
  font-size: 0.95rem;
}

.app {
  display: grid;
  min-width: 100vw;
  min-height: 100vh;
}

/* ---------- Home (clock screen) ---------- */

.home {
  display: grid;
  grid-template-rows: 1fr auto;
  gap: 28px;
  align-items: center;
  justify-items: center;
  padding: 5vh 4vw;
  animation: screen-in 0.15s ease-out;
}

.clock-face {
  width: 100%;
  text-align: center;
}

.clock-logo {
  width: 64px;
  height: auto;
  margin: 0 auto 24px;
  opacity: 0.9;
  object-fit: contain;
}

.clock-time {
  font-size: clamp(7rem, 20vw, 21rem);
  font-weight: 800;
  line-height: 0.9;
  letter-spacing: 0;
  font-variant-numeric: tabular-nums;
  text-shadow: 0 0 80px rgba(242, 234, 217, 0.15);
}

.clock-time .secs {
  color: var(--brass);
}

.clock-date {
  margin-top: 28px;
  font-size: clamp(1.4rem, 3vw, 2.6rem);
  font-weight: 600;
  color: var(--text-2);
  text-transform: uppercase;
  letter-spacing: 0.18em;
}

.start-button {
  min-width: min(68vw, 760px);
  min-height: 96px;
  margin-bottom: 3vh;
  font-size: clamp(2rem, 4vw, 4.5rem);
  border-radius: 999px;
  box-shadow: 0 0 48px var(--brass-glow);
}

.start-button:hover {
  transform: translateY(-2px);
  box-shadow: 0 8px 64px var(--brass-glow);
}

/* ---------- Screens & topbar ---------- */

.screen {
  display: grid;
  grid-template-rows: auto 1fr;
  min-height: 100vh;
  padding: 24px;
  gap: 18px;
  animation: screen-in 0.15s ease-out;
}

.topbar {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 18px;
  padding: 12px 18px;
  border: 1px solid var(--line);
  border-radius: var(--radius-m);
  background: var(--bg-1);
  box-shadow: var(--shadow-1);
}

.topbar button.icon {
  min-height: 44px;
  width: 44px;
  padding: 0;
  border-radius: 50%;
}

.brand {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}

.brand img {
  width: 44px;
  height: 44px;
  object-fit: contain;
}

.title {
  min-width: 0;
}

.title h1 {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: clamp(1.5rem, 2.6vw, 2.4rem);
  font-weight: 700;
  letter-spacing: 0;
}

.title p {
  margin: 2px 0 0;
  color: var(--text-3);
  font-size: 0.95rem;
}

.status-pill {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 30vw;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 8px 16px;
  color: var(--text-2);
  font-size: 0.9rem;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: background 0.3s, color 0.3s, border-color 0.3s;
}

.status-pill svg {
  width: 16px;
  height: 16px;
  flex: none;
}

.status-pill.warn {
  background: var(--warn-tint);
  border-color: var(--warn);
  color: var(--warn);
}

.status-pill.danger {
  background: var(--err-tint);
  border-color: var(--err);
  color: var(--err);
  animation: pulse 1s ease-in-out infinite;
}

/* ---------- Role / staff picker ---------- */

.role-grid,
.staff-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 18px;
  align-content: center;
}

.role-tile,
.staff-tile {
  display: grid;
  align-content: center;
  justify-items: start;
  min-height: 32vh;
  border: 1px solid var(--line);
  border-radius: var(--radius-m);
  padding: 32px;
  background: var(--bg-1);
  text-align: left;
  box-shadow: var(--shadow-1);
  transition: transform 0.12s ease, border-color 0.12s ease, background 0.12s ease;
}

.role-tile:hover:not(:disabled),
.staff-tile:hover:not(:disabled) {
  transform: translateY(-3px);
  border-color: var(--line-2);
  background: var(--bg-2);
}

.role-tile:active,
.staff-tile:active {
  transform: translateY(-1px) scale(0.99);
}

.tile-icon {
  margin-bottom: 20px;
  color: var(--text-3);
  transition: color 0.12s ease;
}

.tile-icon svg {
  width: 44px;
  height: 44px;
}

.role-tile:hover .tile-icon,
.staff-tile:hover .tile-icon {
  color: var(--brass);
}

.role-tile strong,
.staff-tile strong {
  font-size: clamp(2.4rem, 4.5vw, 5rem);
  line-height: 1;
  font-weight: 800;
  letter-spacing: 0;
}

.role-tile span,
.staff-tile span {
  margin-top: 14px;
  color: var(--text-3);
  font-size: 1.05rem;
}

.avatar {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  margin-bottom: 20px;
  background: var(--brass-tint);
  border: 1px solid rgba(217, 164, 65, 0.35);
  color: var(--brass);
  font-size: 1.6rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}

/* ---------- Workspace ---------- */

.workspace {
  display: grid;
  grid-template-columns: 260px 1fr;
  gap: 18px;
  min-height: 0;
}

.side-nav,
.panel {
  border: 1px solid var(--line);
  border-radius: var(--radius-m);
  background: var(--bg-1);
  box-shadow: var(--shadow-1);
}

.side-nav {
  display: grid;
  align-content: start;
  gap: 6px;
  padding: 12px;
  overflow: auto;
}

.side-nav button {
  position: relative;
  justify-content: start;
  gap: 12px;
  min-height: 58px;
  text-align: left;
  color: var(--text-2);
  background: transparent;
  border: none;
  border-radius: var(--radius-s);
  padding: 0 16px;
  font-size: 1rem;
}

.side-nav button svg {
  width: 22px;
  height: 22px;
  flex: none;
}

.side-nav button:hover {
  background: var(--bg-2);
  color: var(--text-1);
}

.side-nav button.active {
  background: var(--brass-tint);
  color: var(--brass);
  font-weight: 700;
}

.side-nav button.active::before {
  content: "";
  position: absolute;
  left: 0;
  top: 12px;
  bottom: 12px;
  width: 3px;
  border-radius: 3px;
  background: var(--brass);
}

.panel {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  display: grid;
  grid-template-rows: auto 1fr;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border-bottom: 1px solid var(--line);
  padding: 16px 20px;
}

.panel-header h2 {
  margin: 0;
  font-size: 1.35rem;
  font-weight: 700;
  letter-spacing: 0;
}

.panel-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  justify-content: flex-end;
}

.panel-body {
  overflow: auto;
  padding: 20px;
  min-height: 0;
}

.panel-body > * {
  animation: fade-in 0.12s ease-out;
}

.panel-body h3 {
  margin: 22px 0 12px;
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--text-2);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.panel-body h3:first-child {
  margin-top: 0;
}

/* ---------- Tables ---------- */

.table-wrap {
  border: 1px solid var(--line);
  border-radius: 12px;
  overflow: hidden;
  background: var(--bg-1);
}

.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.95rem;
  font-variant-numeric: tabular-nums;
}

.table th,
.table td {
  padding: 12px 14px;
  text-align: left;
  vertical-align: top;
  border-bottom: 1px solid var(--line);
}

.table th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--bg-2);
  color: var(--text-2);
  font-weight: 600;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.table tbody tr:nth-child(even) {
  background: rgba(255, 255, 255, 0.02);
}

.table tbody tr:last-child td {
  border-bottom: none;
}

.table tr.clickable {
  cursor: pointer;
}

.table tr.clickable:hover td {
  background: var(--bg-2);
}

.table tr.review td {
  background: var(--err-tint);
}

.table tr.review td:first-child {
  box-shadow: inset 3px 0 0 var(--err);
}

.table em {
  color: var(--text-3);
}

/* ---------- Forms ---------- */

.form-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.form-grid .wide {
  grid-column: 1 / -1;
}

.checkbox-row {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.check {
  display: flex;
  width: auto;
  min-height: 42px;
  align-items: center;
  gap: 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius-s);
  padding: 8px 12px;
  color: var(--text-1);
  background: transparent;
  transition: background 0.12s ease, border-color 0.12s ease;
}

.check:hover {
  background: var(--bg-2);
}

.check:has(input:checked) {
  background: var(--brass-tint);
  border-color: rgba(217, 164, 65, 0.4);
  color: var(--brass);
}

.check input {
  width: auto;
  accent-color: var(--brass);
}

.protected input,
.protected textarea {
  opacity: 0.72;
}

.db-form {
  margin-bottom: 18px;
}

.log-box {
  max-height: 180px;
  overflow: auto;
  border: 1px solid var(--line);
  border-radius: var(--radius-s);
  margin: 12px 0 18px;
  padding: 12px;
  background: var(--bg-inset);
  color: var(--text-2);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
  font-size: 0.92rem;
}

/* ---------- Metrics ---------- */

.metric-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 14px;
  margin-bottom: 18px;
}

.metric {
  border: 1px solid var(--line);
  border-top: 3px solid var(--brass);
  border-radius: var(--radius-s);
  padding: 16px;
  background: var(--bg-1);
}

.metric span {
  color: var(--text-2);
  font-size: 0.78rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.metric strong {
  display: block;
  margin-top: 8px;
  font-size: 2rem;
  font-weight: 800;
  font-variant-numeric: tabular-nums;
}

.metric-ok {
  color: var(--ok);
}

.metric-warn {
  color: var(--warn);
}

.metric-err {
  color: var(--err);
}

/* ---------- Modals ---------- */

.modal-backdrop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.72);
  backdrop-filter: blur(6px);
  padding: 20px;
  z-index: 50;
  animation: fade-in 0.15s ease-out;
}

.modal {
  width: min(560px, 100%);
  border: 1px solid var(--line-2);
  border-radius: var(--radius-l);
  background: var(--bg-1);
  box-shadow: var(--shadow-2);
  animation: modal-in 0.15s ease-out;
}

.modal header,
.modal footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 20px;
}

.modal header {
  border-bottom: 1px solid var(--line);
}

.modal footer {
  border-top: 1px solid var(--line);
}

.modal h2 {
  margin: 0;
  font-weight: 700;
}

.modal .body {
  display: grid;
  gap: 14px;
  padding: 20px;
}

.message {
  min-height: 24px;
  color: var(--text-2);
}

.message.error {
  color: var(--err);
}

.empty {
  display: grid;
  min-height: 50vh;
  place-items: center;
  color: var(--text-3);
  font-size: clamp(1.6rem, 4vw, 3rem);
  font-weight: 600;
  text-align: center;
}

/* ---------- Status tags ---------- */

.tag {
  display: inline-block;
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 0.8rem;
  font-weight: 600;
  letter-spacing: 0.02em;
  border: 1px solid transparent;
}

.tag-ok {
  background: var(--ok-tint);
  color: var(--ok);
  border-color: rgba(90, 164, 105, 0.35);
}

.tag-warn {
  background: var(--warn-tint);
  color: var(--warn);
  border-color: rgba(224, 123, 57, 0.35);
}

.tag-err {
  background: var(--err-tint);
  color: var(--err);
  border-color: rgba(217, 92, 92, 0.35);
}

/* ---------- Scan status bar ---------- */

.scan-status {
  margin-top: 0.5em;
  padding: 10px 14px;
  border-radius: var(--radius-s);
  font-size: 0.95rem;
  text-align: center;
  border: 1px solid var(--line);
  border-left-width: 4px;
  background: var(--bg-2);
  color: var(--text-1);
  transition: background 0.2s ease, border-color 0.2s ease, color 0.2s ease;
}

.scan-status.info {
  border-left-color: var(--brass);
  background: var(--brass-tint);
}

.scan-status.warn {
  border-left-color: var(--warn);
  background: var(--warn-tint);
  color: var(--warn);
}

.scan-status.ok {
  border-left-color: var(--ok);
  background: var(--ok-tint);
  color: var(--ok);
}

.scan-status.err {
  border-left-color: var(--err);
  background: var(--err-tint);
  color: var(--err);
}

.cancel-text {
  color: var(--err);
  font-weight: 600;
}

/* ---------- Auth fingerprint ---------- */

.auth-fp-icon {
  position: relative;
  display: grid;
  place-items: center;
  width: 128px;
  height: 128px;
  margin: 8px auto;
  cursor: pointer;
}

.auth-fp-icon img {
  width: 72px;
  height: 72px;
  object-fit: contain;
  filter: invert(1) brightness(0.85);
  position: relative;
  z-index: 1;
  transition: filter 0.3s;
}

.auth-fp-icon .ring {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 2px solid var(--brass);
  opacity: 0;
  pointer-events: none;
}

.auth-fp-icon.scanning .ring {
  animation: ring-pulse 2s ease-out infinite;
}

.auth-fp-icon.scanning .ring.r2 {
  animation-delay: 1s;
}

.auth-fp-icon.scanning img {
  animation: fp-glow 1.5s ease-in-out infinite;
}

/* ---------- Misc ---------- */

.brochure-disabled {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}

#cornice-search-results {
  font-size: 0.85em;
  color: var(--text-3);
}

/* ---------- Motion ---------- */

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.5;
  }
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes screen-in {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

@keyframes modal-in {
  from {
    opacity: 0;
    transform: translateY(12px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

@keyframes ring-pulse {
  0% {
    transform: scale(0.75);
    opacity: 0.8;
  }
  100% {
    transform: scale(1.25);
    opacity: 0;
  }
}

@keyframes fp-glow {
  0%,
  100% {
    filter: invert(1) brightness(0.85) drop-shadow(0 0 4px rgba(217, 164, 65, 0.4));
  }
  50% {
    filter: invert(1) brightness(1.15) drop-shadow(0 0 16px rgba(217, 164, 65, 0.8));
  }
}

@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}

@media (max-width: 900px) {
  .role-grid,
  .staff-grid,
  .workspace,
  .metric-row,
  .form-grid {
    grid-template-columns: 1fr;
  }

  .role-tile,
  .staff-tile {
    min-height: 20vh;
  }

  .status-pill {
    display: none;
  }
}
```

- [ ] **Step 2: Verify CSS structure**

```bash
python3 - <<'EOF'
src = open("ui/styles.css").read()
assert src.count("{") == src.count("}"), f"unbalanced braces: {src.count('{')} vs {src.count('}')}"
required = [
  "--bg-0", "--bg-inset", "--brass", "--warn", "--err", "--ok",
  "@font-face", "inter-var.woff2",
  ".clock-logo", ".secs", ".tile-icon", ".avatar", ".table-wrap",
  ".tag-ok", ".tag-warn", ".tag-err",
  ".scan-status", ".scan-status.info", ".scan-status.warn", ".scan-status.ok", ".scan-status.err",
  ".metric-ok", ".metric-warn", ".metric-err",
  ".cancel-text", ".auth-fp-icon .ring",
  "prefers-reduced-motion", "backdrop-filter",
]
missing = [r for r in required if r not in src]
assert not missing, f"missing rules/tokens: {missing}"
print(f"styles.css OK: {src.count('{')} rules, all required selectors present")
EOF
```
Expected: `styles.css OK: ...` with no assertion errors.

- [ ] **Step 3: Visual check (user)**

Ask the user: open `ui/index.html` in a browser (double-click or `xdg-open ui/index.html`). Expected: home screen shows the new warm charcoal background with faint grain, Inter font, cream clock, brass pill Start button with glow. The role menu (click Start) shows restyled tiles. If the font didn't load, the clock falls back to system sans — flag that.

- [ ] **Step 4: Commit**

```bash
git add ui/styles.css
git commit -m "rewrite styles.css as polished plaster design system"
```

---

### Task 3: Home clock, role tiles, staff avatars (app.js)

**Files:**
- Modify: `ui/js/app.js` (renderHome, tickClock, renderRoleMenu, renderStaffPicker + import)

**Interfaces:**
- Consumes: `icon(name)` from `ui/js/icons.js` (Task 1); CSS classes `.clock-logo`, `.secs`, `.tile-icon`, `.avatar` (Task 2).
- Produces: nothing new for later tasks.

- [ ] **Step 1: Add the icons import**

In `ui/js/app.js` line 9, after the `auth.js` import, add:

```js
import { icon } from "./icons.js";
```

Full import block becomes:

```js
import {
  escapeHtml,
  formatAction,
  invoke,
  setBusy,
  todayIso,
  weekStartIso,
} from "./api.js";
import { chooseClockAction, requestAuth } from "./auth.js";
import { icon } from "./icons.js";
```

- [ ] **Step 2: Replace `renderHome` (lines 75-87)**

Old:

```js
function renderHome() {
  app.innerHTML = `
    <section class="home">
      <div class="clock-face">
        <div class="clock-time" data-time>00:00:00</div>
        <div class="clock-date" data-date>00/00/0000 Monday</div>
      </div>
      <button class="primary start-button" data-start>Start</button>
    </section>
  `;
  app.querySelector("[data-start]").addEventListener("click", renderRoleMenu);
  tickClock();
}
```

New:

```js
function renderHome() {
  const logo = state.logoDataUrl || "./assets/HPS.png";
  app.innerHTML = `
    <section class="home">
      <div class="clock-face">
        <img class="clock-logo" src="${logo}" alt="HPS" />
        <div class="clock-time"><span data-time-main>00:00:</span><span class="secs" data-time-secs>00</span></div>
        <div class="clock-date" data-date>00/00/0000 Monday</div>
      </div>
      <button class="primary start-button" data-start>Start</button>
    </section>
  `;
  app.querySelector("[data-start]").addEventListener("click", renderRoleMenu);
  tickClock();
}
```

- [ ] **Step 3: Replace `tickClock` (lines 89-114)**

Old:

```js
let clockTimer = null;
let sessionTimer = null;
function tickClock() {
  if (clockTimer) clearInterval(clockTimer);
  const timeNode = app.querySelector("[data-time]");
  const dateNode = app.querySelector("[data-date]");
  const update = () => {
    const now = new Date();
    if (timeNode) {
      timeNode.textContent = now.toLocaleTimeString("en-AU", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      });
    }
    if (dateNode) {
      const day = now.toLocaleDateString("en-AU", { weekday: "long" });
      const dd = String(now.getDate()).padStart(2, "0");
      const mm = String(now.getMonth() + 1).padStart(2, "0");
      dateNode.textContent = `${dd}/${mm}/${now.getFullYear()} ${day}`;
    }
  };
  update();
  clockTimer = setInterval(update, 1000);
}
```

New:

```js
let clockTimer = null;
let sessionTimer = null;
function tickClock() {
  if (clockTimer) clearInterval(clockTimer);
  const timeMain = app.querySelector("[data-time-main]");
  const timeSecs = app.querySelector("[data-time-secs]");
  const dateNode = app.querySelector("[data-date]");
  const update = () => {
    const now = new Date();
    if (timeMain || timeSecs) {
      const t = now.toLocaleTimeString("en-AU", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      });
      if (timeMain) timeMain.textContent = `${t.slice(0, 5)}:`;
      if (timeSecs) timeSecs.textContent = t.slice(6, 8);
    }
    if (dateNode) {
      const day = now.toLocaleDateString("en-AU", { weekday: "long" });
      const dd = String(now.getDate()).padStart(2, "0");
      const mm = String(now.getMonth() + 1).padStart(2, "0");
      dateNode.textContent = `${dd}/${mm}/${now.getFullYear()} ${day}`;
    }
  };
  update();
  clockTimer = setInterval(update, 1000);
}
```

- [ ] **Step 4: Replace the role tiles in `renderRoleMenu` (lines 190-199)**

Old:

```js
  app.innerHTML = screenShell(
    "Choose Access",
    "Hopkins Plaster Studio",
    `
      <button class="role-tile" data-role="admin"><strong>Admin</strong><span>Full control</span></button>
      <button class="role-tile" data-role="staff"><strong>Staff</strong><span>Clocking and daily logs</span></button>
      <button class="role-tile brochure-disabled" disabled title="Coming soon"><strong>Brochure</strong><span>Coming soon</span></button>
    `,
    "role-grid",
  );
```

New:

```js
  app.innerHTML = screenShell(
    "Choose Access",
    "Hopkins Plaster Studio",
    `
      <button class="role-tile" data-role="admin"><span class="tile-icon">${icon("shield")}</span><strong>Admin</strong><span>Full control</span></button>
      <button class="role-tile" data-role="staff"><span class="tile-icon">${icon("user")}</span><strong>Staff</strong><span>Clocking and daily logs</span></button>
      <button class="role-tile brochure-disabled" disabled title="Coming soon"><span class="tile-icon">${icon("book")}</span><strong>Brochure</strong><span>Coming soon</span></button>
    `,
    "role-grid",
  );
```

- [ ] **Step 5: Replace the staff tiles in `renderStaffPicker` (lines 231-242)**

Old:

```js
  app.innerHTML = screenShell(
    "Staff",
    "Choose your name",
    state.staff
      .map(
        (employee) => `
        <button class="staff-tile" data-employee="${escapeHtml(employee.id)}">
          <strong>${escapeHtml(employee.name)}</strong>
          <span>${escapeHtml(employee.id)}</span>
        </button>
      `,
      )
      .join(""),
    "staff-grid",
  );
```

New:

```js
  app.innerHTML = screenShell(
    "Staff",
    "Choose your name",
    state.staff
      .map((employee) => {
        const initials = employee.name
          .split(/\s+/)
          .filter(Boolean)
          .slice(0, 2)
          .map((word) => word[0].toUpperCase())
          .join("");
        return `
        <button class="staff-tile" data-employee="${escapeHtml(employee.id)}">
          <span class="avatar">${escapeHtml(initials)}</span>
          <strong>${escapeHtml(employee.name)}</strong>
          <span>${escapeHtml(employee.id)}</span>
        </button>
      `;
      })
      .join(""),
    "staff-grid",
  );
```

- [ ] **Step 6: Verify**

```bash
grep -n 'data-time-main\|data-time-secs' ui/js/app.js
grep -n 'tile-icon\|class="avatar"' ui/js/app.js
grep -n 'import { icon } from "./icons.js"' ui/js/app.js
! grep -n 'data-time>' ui/js/app.js
```
Expected: first three greps each print matches (2, 4, 1 lines); the last grep prints nothing (old `data-time` attribute gone).

- [ ] **Step 7: Visual check (user)**

Ask the user to open `ui/index.html` in a browser. Expected: logo above the clock, seconds rendered in brass and ticking, date in letter-spaced small caps, Start as a glowing brass pill. Click Start → role tiles show shield/person/book icons that turn brass on hover. (Staff picker needs the Tauri app — verify in Task 6.)

- [ ] **Step 8: Commit**

```bash
git add ui/js/app.js
git commit -m "restyle home clock, role tiles, staff avatars"
```

---

### Task 4: Workspace chrome — topbar, nav icons, table wrap, cell escape rule (app.js)

**Files:**
- Modify: `ui/js/app.js` (topbar, workspaceShell, table, cellLooksHtml)

**Interfaces:**
- Consumes: `icon(name)` (Task 1); CSS `.status-pill svg`, `.side-nav button svg`, `.side-nav button.active::before`, `.table-wrap` (Task 2).
- Produces: `tabIcons` map used by `workspaceShell` for both admin and staff workspaces.

- [ ] **Step 1: Replace `topbar` (lines 1746-1763)**

Old:

```js
function topbar(title, subtitle) {
  const logo = state.logoDataUrl || "./assets/HPS.png";
  return `
    <header class="topbar">
      <button class="icon ghost" data-back title="Back"><img src="./assets/noun-arrow-back-2352160.svg" alt="Back" width="24" height="24" /></button>
      <div class="brand">
        <img src="${logo}" alt="" />
        <div class="title">
          <h1>${escapeHtml(title)}</h1>
          <p>${escapeHtml(subtitle)}</p>
        </div>
      </div>
      <div class="status-pill" data-status-pill title="${escapeHtml(state.status?.database_path || "")}">
        Session Timer<span data-session-remaining></span>
      </div>
    </header>
  `;
}
```

New:

```js
function topbar(title, subtitle) {
  const logo = state.logoDataUrl || "./assets/HPS.png";
  return `
    <header class="topbar">
      <button class="icon ghost" data-back title="Back">${icon("back-arrow")}</button>
      <div class="brand">
        <img src="${logo}" alt="" />
        <div class="title">
          <h1>${escapeHtml(title)}</h1>
          <p>${escapeHtml(subtitle)}</p>
        </div>
      </div>
      <div class="status-pill" data-status-pill title="${escapeHtml(state.status?.database_path || "")}">
        ${icon("clock", 16)}
        <span>Session<span data-session-remaining></span></span>
      </div>
    </header>
  `;
}
```

Note: `startSessionTimer` in app.js toggles `warn`/`danger` classes on `[data-status-pill]` and writes `[data-session-remaining]` — both attributes are preserved, so no JS change there.

- [ ] **Step 2: Replace `workspaceShell` (lines 1721-1744)**

Old:

```js
function workspaceShell(title, subtitle, tabs, active) {
  return `
    <section class="screen">
      ${topbar(title, subtitle)}
      <div class="workspace">
        <nav class="side-nav">
          ${tabs
            .map(
              ([id, label]) => `
                <button data-tab="${id}" class="${active === id ? "active" : ""}">
                  ${escapeHtml(label)}
                </button>
              `,
            )
            .join("")}
        </nav>
        <section class="panel">
          <div class="panel-header"><h2 data-panel-title></h2><div class="panel-actions" data-panel-actions></div></div>
          <div class="panel-body" data-panel-body></div>
        </section>
      </div>
    </section>
  `;
}
```

New:

```js
const tabIcons = {
  alerts: "bell",
  employees: "users",
  enroll: "fingerprint",
  payroll: "dollar",
  dispatch: "truck",
  cornice_stock: "box",
  mould_inventory: "layers",
  stock: "box",
  rates: "gauge",
  time: "clock",
  logs: "list",
  database: "database",
  cornice: "box",
  moulds: "layers",
  production: "gauge",
  deliveries: "truck",
  cornice_stock_ro: "box",
  overstock: "box",
};

function workspaceShell(title, subtitle, tabs, active) {
  return `
    <section class="screen">
      ${topbar(title, subtitle)}
      <div class="workspace">
        <nav class="side-nav">
          ${tabs
            .map(
              ([id, label]) => `
                <button data-tab="${id}" class="${active === id ? "active" : ""}">
                  ${icon(tabIcons[id] || "list")}${escapeHtml(label)}
                </button>
              `,
            )
            .join("")}
        </nav>
        <section class="panel">
          <div class="panel-header"><h2 data-panel-title></h2><div class="panel-actions" data-panel-actions></div></div>
          <div class="panel-body" data-panel-body></div>
        </section>
      </div>
    </section>
  `;
}
```

- [ ] **Step 3: Wrap tables in `.table-wrap` — replace `table` (lines 1771-1789)**

Old:

```js
function table(headers, rows) {
  if (!rows.length) return `<div class="message">No records</div>`;
  return `
    <table class="table">
      <thead><tr>${headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("")}</tr></thead>
      <tbody>
        ${rows
          .map(
            (row) => `
              <tr class="${row.review ? "review" : ""} ${row.clickable ? "clickable" : ""}" ${row.attrs || ""}>
                ${row.cells.map((cell) => `<td>${cellLooksHtml(cell) ? cell : escapeHtml(cell)}</td>`).join("")}
              </tr>
            `,
          )
          .join("")}
      </tbody>
    </table>
  `;
}
```

New:

```js
function table(headers, rows) {
  if (!rows.length) return `<div class="message">No records</div>`;
  return `
    <div class="table-wrap">
      <table class="table">
        <thead><tr>${headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("")}</tr></thead>
        <tbody>
          ${rows
            .map(
              (row) => `
              <tr class="${row.review ? "review" : ""} ${row.clickable ? "clickable" : ""}" ${row.attrs || ""}>
                ${row.cells.map((cell) => `<td>${cellLooksHtml(cell) ? cell : escapeHtml(cell)}</td>`).join("")}
              </tr>
            `,
            )
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}
```

- [ ] **Step 4: Extend `cellLooksHtml` (lines 1791-1793)**

Old:

```js
function cellLooksHtml(value) {
  return typeof value === "string" && value.trim().startsWith("<button");
}
```

New:

```js
function cellLooksHtml(value) {
  const v = typeof value === "string" ? value.trim() : "";
  return v.startsWith("<button") || v.startsWith('<span class="tag');
}
```

- [ ] **Step 5: Verify**

```bash
grep -n 'icon("back-arrow")' ui/js/app.js
grep -n 'const tabIcons' ui/js/app.js
grep -n 'table-wrap' ui/js/app.js
grep -n 'span class="tag' ui/js/app.js
grep -c 'data-status-pill\|data-session-remaining' ui/js/app.js
```
Expected: first four greps print 1 line each; last grep prints `4` (two lines in topbar markup + two lines in startSessionTimer).

- [ ] **Step 6: Commit**

```bash
git add ui/js/app.js
git commit -m "restyle workspace chrome: topbar chip, nav icons, table wrap"
```

---

### Task 5: Replace all inline light-mode colors with tag/scan-status classes

**Files:**
- Modify: `ui/js/app.js` (enroll panel, payroll, dispatch ×2, staff payroll)
- Modify: `ui/js/auth.js` (auth modal + all fp status updates)

**Interfaces:**
- Consumes: CSS `.tag-ok/.tag-warn/.tag-err`, `.scan-status .info/.warn/.ok/.err`, `.metric-ok/.metric-warn/.metric-err`, `.cancel-text`, `.auth-fp-icon .ring` (Task 2); `icon("x")` (Task 1).
- Produces: no inline hex colors remain anywhere in `ui/` (verified by grep in Step 9).

- [ ] **Step 1: Enroll panel — scan status element (app.js line 463)**

Old:

```js
      <div id="enroll-scan-status" class="message" style="display:none;margin-top:0.5em;padding:0.5em 0.75em;border-radius:4px;font-size:0.9em;text-align:center;"></div>
```

New:

```js
      <div id="enroll-scan-status" class="scan-status" style="display:none"></div>
```

- [ ] **Step 2: Enroll panel — cancel button (app.js lines 496-501)**

Old:

```js
      const cancelEl = document.createElement("button");
      cancelEl.type = "button";
      cancelEl.className = "ghost";
      cancelEl.setAttribute("data-cancel-enroll", "");
      cancelEl.style.cssText = "color:#e74c3c;font-weight:600;margin-left:8px;";
      cancelEl.textContent = "Cancel";
```

New:

```js
      const cancelEl = document.createElement("button");
      cancelEl.type = "button";
      cancelEl.className = "ghost cancel-text";
      cancelEl.setAttribute("data-cancel-enroll", "");
      cancelEl.textContent = "Cancel";
```

- [ ] **Step 3: Enroll panel — scan status init (app.js lines 513-521)**

Old:

```js
      const scanStatus = document.getElementById("enroll-scan-status");
      let enrollAttempts = 0;
      let lastQuality = null;
      if (scanStatus) {
        scanStatus.style.display = "";
        scanStatus.textContent = "Place your finger on the scanner...";
        scanStatus.style.background = "#eaf7ff";
        scanStatus.style.color = "#2c3e50";
      }
```

New:

```js
      const scanStatus = document.getElementById("enroll-scan-status");
      let enrollAttempts = 0;
      let lastQuality = null;
      if (scanStatus) {
        scanStatus.style.display = "";
        scanStatus.className = "scan-status info";
        scanStatus.textContent = "Place your finger on the scanner...";
      }
```

- [ ] **Step 4: Enroll panel — poll loop status updates (app.js lines 535-566)**

Old:

```js
            if (raw.startsWith("RETRY|")) {
              enrollAttempts++;
              lastQuality = "retry";
              const reason = raw.split("|").slice(1).join("|");
              if (scanStatus) {
                scanStatus.textContent = `Attempt ${enrollAttempts}: ${mapRetryReason(reason)}`;
                scanStatus.style.background = "#fff3e6";
                scanStatus.style.color = "#c0571a";
              }
            } else if (raw.startsWith("PROGRESS|")) {
              lastQuality = "good";
              const [, completed, total] = raw.split("|");
              if (scanStatus) {
                scanStatus.textContent = `✓ Good scan — stage ${completed}/${total} captured`;
                scanStatus.style.background = "#e6fff0";
                scanStatus.style.color = "#1a7a42";
              }
            } else if (raw.startsWith("READY|")) {
              if (scanStatus && !lastQuality) {
                scanStatus.textContent = "Scanner ready — place your finger now";
                scanStatus.style.background = "#eaf7ff";
                scanStatus.style.color = "#2c3e50";
              }
            } else if (raw.startsWith("TEMPLATE|")) {
              const [, num, total] = raw.split("|");
              if (scanStatus) {
                scanStatus.textContent = `Enrolling template ${num} of ${total} — place your finger`;
                scanStatus.style.background = "#eaf7ff";
                scanStatus.style.color = "#2c3e50";
              }
            }
```

New:

```js
            if (raw.startsWith("RETRY|")) {
              enrollAttempts++;
              lastQuality = "retry";
              const reason = raw.split("|").slice(1).join("|");
              if (scanStatus) {
                scanStatus.className = "scan-status warn";
                scanStatus.textContent = `Attempt ${enrollAttempts}: ${mapRetryReason(reason)}`;
              }
            } else if (raw.startsWith("PROGRESS|")) {
              lastQuality = "good";
              const [, completed, total] = raw.split("|");
              if (scanStatus) {
                scanStatus.className = "scan-status ok";
                scanStatus.textContent = `✓ Good scan — stage ${completed}/${total} captured`;
              }
            } else if (raw.startsWith("READY|")) {
              if (scanStatus && !lastQuality) {
                scanStatus.className = "scan-status info";
                scanStatus.textContent = "Scanner ready — place your finger now";
              }
            } else if (raw.startsWith("TEMPLATE|")) {
              const [, num, total] = raw.split("|");
              if (scanStatus) {
                scanStatus.className = "scan-status info";
                scanStatus.textContent = `Enrolling template ${num} of ${total} — place your finger`;
              }
            }
```

- [ ] **Step 5: Admin payroll — unresolved metric + status tag (app.js lines 949-955, 972)**

Old (metric):

```js
        <div class="metric"><span>Unresolved</span><strong style="color:${unresolved.length ? '#e55' : 'inherit'}">${unresolved.length}</strong></div>
```

New:

```js
        <div class="metric"><span>Unresolved</span><strong class="${unresolved.length ? "metric-err" : ""}">${unresolved.length}</strong></div>
```

Old (status cell):

```js
            `<span style="color:${p.status === 'final' ? '#5a5' : p.status === 'unresolved' ? '#e55' : '#da5'}">${escapeHtml(p.status)}</span>`,
```

New:

```js
            `<span class="tag ${p.status === 'final' ? 'tag-ok' : p.status === 'unresolved' ? 'tag-err' : 'tag-warn'}">${escapeHtml(p.status)}</span>`,
```

- [ ] **Step 6: Admin dispatch — status tag (app.js line 1185)**

Old:

```js
            `<span style="color:${o.status === 'delivered' ? '#5a5' : o.status === 'pending' ? '#e55' : '#da5'}">${escapeHtml(o.status)}</span>`,
```

New:

```js
            `<span class="tag ${o.status === 'delivered' ? 'tag-ok' : o.status === 'pending' ? 'tag-err' : 'tag-warn'}">${escapeHtml(o.status)}</span>`,
```

- [ ] **Step 7: Driver dispatch — status tag (app.js line 1583)**

Old (identical string to Step 6 — it is the second occurrence in the file, inside `renderDriverDispatchView`):

```js
            `<span style="color:${o.status === 'delivered' ? '#5a5' : o.status === 'pending' ? '#e55' : '#da5'}">${escapeHtml(o.status)}</span>`,
```

New:

```js
            `<span class="tag ${o.status === 'delivered' ? 'tag-ok' : o.status === 'pending' ? 'tag-err' : 'tag-warn'}">${escapeHtml(o.status)}</span>`,
```

Note: after Step 6, only one occurrence of the old string remains, so the edit tool's unique-match requirement is satisfied. If editing manually, replace the one in `renderDriverDispatchView`.

- [ ] **Step 8: Staff payroll — status metric + table wrap (app.js lines 1671-1690)**

Old (metric):

```js
        <div class="metric"><span>Status</span><strong style="color:${payroll.status === 'final' ? '#5a5' : payroll.status === 'unresolved' ? '#e55' : '#da5'}">${escapeHtml(payroll.status)}</strong></div>
```

New:

```js
        <div class="metric"><span>Status</span><strong class="${payroll.status === 'final' ? 'metric-ok' : payroll.status === 'unresolved' ? 'metric-err' : 'metric-warn'}">${escapeHtml(payroll.status)}</strong></div>
```

Old (raw table open/close):

```js
    body += `<h3>Pay Breakdown</h3>`;
    body += `<table class="table"><tbody>`;
```

New:

```js
    body += `<h3>Pay Breakdown</h3>`;
    body += `<div class="table-wrap"><table class="table"><tbody>`;
```

Old (raw table close):

```js
    body += `</tbody></table>`;
```

New:

```js
    body += `</tbody></table></div>`;
```

- [ ] **Step 9: auth.js — import icon**

Old (line 1):

```js
import { escapeHtml, invoke, setBusy } from "./api.js";
```

New:

```js
import { escapeHtml, invoke, setBusy } from "./api.js";
import { icon } from "./icons.js";
```

- [ ] **Step 10: auth.js — auth modal template (lines 47-71)**

Old:

```js
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">X</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employeeLabel)}</div>
            <div id="auth-fp-icon" class="auth-fp-icon scanning">
              <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="80" height="80" />
            </div>
            <div id="auth-fp-status" class="message" style="margin-top:0.5em;font-size:0.9em;text-align:center;">Scanning…</div>
            <label id="auth-password-label" style="display:none">
              Password
              <input data-password type="password" autocomplete="current-password" placeholder="Enter password…" />
            </label>
            <div class="message" data-message></div>
          </div>
          <footer>
            <button class="primary" data-password-submit style="display:none">Continue</button>
          </footer>
        </section>
      </div>
    `;
```

New:

```js
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <section class="modal" role="dialog" aria-modal="true">
          <header>
            <h2>${escapeHtml(title)}</h2>
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
          </header>
          <div class="body">
            <div class="message">${escapeHtml(employeeLabel)}</div>
            <div id="auth-fp-icon" class="auth-fp-icon scanning">
              <span class="ring"></span>
              <span class="ring r2"></span>
              <img src="./assets/noun-fingerprint-1377758.svg" alt="Fingerprint" width="72" height="72" />
            </div>
            <div id="auth-fp-status" class="scan-status info">Scanning…</div>
            <label id="auth-password-label" style="display:none">
              Password
              <input data-password type="password" autocomplete="current-password" placeholder="Enter password…" />
            </label>
            <div class="message" data-message></div>
          </div>
          <footer>
            <button class="primary" data-password-submit style="display:none">Continue</button>
          </footer>
        </section>
      </div>
    `;
```

- [ ] **Step 11: auth.js — showPasswordFallback (lines 81-89)**

Old:

```js
    const showPasswordFallback = () => {
      if (passwordVisible) return;
      passwordVisible = true;
      passwordLabel.style.display = "";
      passwordButton.style.display = "";
      fpStatus.textContent = "5/5 tries failed — try again, or use password below.";
      fpStatus.style.color = "#c55";
      passwordInput.focus();
    };
```

New:

```js
    const showPasswordFallback = () => {
      if (passwordVisible) return;
      passwordVisible = true;
      passwordLabel.style.display = "";
      passwordButton.style.display = "";
      fpStatus.className = "scan-status err";
      fpStatus.textContent = "5/5 tries failed — try again, or use password below.";
      passwordInput.focus();
    };
```

- [ ] **Step 12: auth.js — scan start (lines 102-104)**

Old:

```js
      fpStatus.textContent = "Starting scan…";
      fpStatus.style.color = "#2c3e50";
      fpStatus.style.background = "#eaf7ff";
```

New:

```js
      fpStatus.className = "scan-status info";
      fpStatus.textContent = "Starting scan…";
```

- [ ] **Step 13: auth.js — ATTEMPT/RETRY updates (lines 124-134)**

Old:

```js
              if (raw.startsWith("ATTEMPT|")) {
                const parts = raw.split("|");
                scanAttempts = parseInt(parts[1]) || scanAttempts;
                fpStatus.textContent = `Scan attempt ${scanAttempts}/${parts[2]}: waiting for finger…`;
                fpStatus.style.background = "#eaf7ff";
                fpStatus.style.color = "#2c3e50";
              } else if (raw.startsWith("RETRY|")) {
                lastRetryReason = raw.split("|").slice(1).join("|");
                fpStatus.textContent = `⚠ Attempt ${scanAttempts}: ${mapRetryReason(lastRetryReason)}`;
                fpStatus.style.background = "#fff3e6";
                fpStatus.style.color = "#c0571a";
              } else if (raw.startsWith("ERROR|")) {
```

New:

```js
              if (raw.startsWith("ATTEMPT|")) {
                const parts = raw.split("|");
                scanAttempts = parseInt(parts[1]) || scanAttempts;
                fpStatus.className = "scan-status info";
                fpStatus.textContent = `Scan attempt ${scanAttempts}/${parts[2]}: waiting for finger…`;
              } else if (raw.startsWith("RETRY|")) {
                lastRetryReason = raw.split("|").slice(1).join("|");
                fpStatus.className = "scan-status warn";
                fpStatus.textContent = `⚠ Attempt ${scanAttempts}: ${mapRetryReason(lastRetryReason)}`;
              } else if (raw.startsWith("ERROR|")) {
```

- [ ] **Step 14: auth.js — wrong fingerprint (lines 143-146)**

Old:

```js
            if (employee && status.employee.id !== employee.id) {
              fpStatus.textContent = `Wrong fingerprint — this session is for ${employee.name}`;
              fpStatus.style.background = "#f8d7da";
              fpStatus.style.color = "#842029";
```

New:

```js
            if (employee && status.employee.id !== employee.id) {
              fpStatus.className = "scan-status err";
              fpStatus.textContent = `Wrong fingerprint — this session is for ${employee.name}`;
```

- [ ] **Step 15: auth.js — job-failed branch (lines 171-178)**

Old:

```js
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        } else {
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
          fpStatus.style.background = "#fff3e6";
          fpStatus.style.color = "#c55";
        }
```

New:

```js
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        } else {
          fpStatus.className = "scan-status err";
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${displayReason}`;
        }
```

- [ ] **Step 16: auth.js — catch branch (lines 187-194)**

Old:

```js
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
        } else {
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
          fpStatus.style.background = "#fff3e6";
          fpStatus.style.color = "#c55";
        }
```

New:

```js
        if (fpFailures >= maxFpFailures) {
          showPasswordFallback();
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
        } else {
          fpStatus.className = "scan-status err";
          fpStatus.textContent = `${fpFailures}/5 tries failed: ${reason}`;
        }
```

- [ ] **Step 17: auth.js — clock action modal close button (line 249)**

Old:

```js
            <button class="icon ghost" data-close title="Close">X</button>
```

New:

```js
            <button class="icon ghost" data-close title="Close">${icon("x")}</button>
```

Note: after Step 10, only one `>X</button>` occurrence remains (the clock action modal), so this matches uniquely.

- [ ] **Step 18: Verify — no inline light-mode colors remain**

```bash
grep -rnE "#(eaf7ff|2c3e50|fff3e6|c0571a|e6fff0|1a7a42|e74c3c|c55|5a5|da5|e55|fff3cd|ffc107|664d03|842029|dc3545|666|101010|211f1d|1d1b19|ffb0b7|3f7d58|4f9469|c0832b|ba3b46|587792|23211f|2e2b27|171717|514b43)\b" --include="*.js" --include="*.css" --include="*.html" ui/
```
Expected: NO output (grep exits 1). If anything prints, replace it per the mapping in the spec's "Inline color cleanup" table and re-run. (Note: `src-tauri/tauri.conf.json` still contains `#171717` until Task 6 — it is intentionally excluded here and covered by the Task 6 sweep.)

Also verify the JS is still well-formed (balanced template literals):

```bash
python3 - <<'EOF'
for f in ("ui/js/app.js", "ui/js/auth.js", "ui/js/icons.js", "ui/js/api.js"):
    src = open(f).read()
    assert src.count("`") % 2 == 0, f"{f}: unbalanced backticks"
    assert src.count("{") == src.count("}"), f"{f}: unbalanced braces"
    assert src.count("(") == src.count(")"), f"{f}: unbalanced parens"
print("JS structure OK")
EOF
```
Expected: `JS structure OK`

- [ ] **Step 19: Commit**

```bash
git add ui/js/app.js ui/js/auth.js
git commit -m "replace inline light-mode colors with tag and scan-status classes"
```

---

### Task 6: Window background + final verification sweep

**Files:**
- Modify: `src-tauri/tauri.conf.json` (one value)

**Interfaces:**
- Consumes: everything from Tasks 1–5.
- Produces: final state of the feature.

- [ ] **Step 1: Match window background to the new page background**

In `src-tauri/tauri.conf.json`, inside `app.windows[0]`:

Old:

```json
        "backgroundColor": "#171717"
```

New:

```json
        "backgroundColor": "#141210"
```

- [ ] **Step 2: cargo check**

Run: `cd src-tauri && cargo check`
Expected: finishes with no errors (no Rust code changed, so this should be a no-op compile).

- [ ] **Step 3: Final forbidden-color sweep**

```bash
grep -rnE "#(eaf7ff|2c3e50|fff3e6|c0571a|e6fff0|1a7a42|e74c3c|c55|5a5|da5|e55|fff3cd|ffc107|664d03|842029|dc3545|666|101010|211f1d|1d1b19|ffb0b7|3f7d58|4f9469|c0832b|ba3b46|587792|23211f|2e2b27|171717|514b43)\b" --include="*.js" --include="*.css" --include="*.html" --include="*.json" ui/ src-tauri/tauri.conf.json
```
Expected: NO output.

- [ ] **Step 4: Final visual pass (user)**

Ask the user to run the app (`cargo tauri dev` in `src-tauri`, or the release binary) and check, in order:

1. Home: grain texture, brass glow, Inter clock with brass seconds, pill Start button.
2. Role menu → Staff → pick a name: staff tiles show initials avatars.
3. Admin login (fingerprint or password): modal has blurred backdrop, pulsing brass fingerprint rings, dark-mode scan status bar.
4. Admin workspace: icon side nav with brass active state, session chip in topbar, zebra tables in rounded wraps, brass-accented metric cards.
5. Payroll + Dispatch tabs: status chips (tag-ok/warn/err), no stray colored text.
6. Enroll tab: start an enrollment — scan status bar cycles info/warn/ok with left accent bar; Cancel button is clay-red text.
7. Staff workspace (log in as a staff member): clock tab, cornice log with autocomplete, My Payroll breakdown table.

If anything looks off, fix forward (adjust the CSS token or rule, re-run the relevant verify step, amend the task's commit).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "match window background to new theme"
```

---

## Plan Self-Review (completed by planner)

1. **Spec coverage:** palette tokens → Task 2; typography/font → Tasks 1–2; home → Task 3; role/staff picker → Task 3; workspace chrome → Task 4; tables → Task 4; forms/metrics → Task 2 (CSS) + Task 5 (classes); modals/auth → Task 2 (CSS) + Task 5 (markup); status tags → Task 5; motion → Task 2; inline color cleanup → Task 5; window bg → Task 6; verification steps → Tasks 2, 3, 5, 6. All spec sections mapped.
2. **Placeholder scan:** none — every step contains exact code or exact commands.
3. **Type/name consistency:** `icon(name)` signature used identically in Tasks 3–5; CSS class names in Tasks 3–5 match Task 2 selectors exactly (`.tile-icon`, `.avatar`, `.table-wrap`, `.tag-ok/.tag-warn/.tag-err`, `.scan-status .info/.warn/.ok/.err`, `.metric-ok/.metric-warn/.metric-err`, `.cancel-text`, `.ring`); `data-*` attributes preserved per Global Constraints.
