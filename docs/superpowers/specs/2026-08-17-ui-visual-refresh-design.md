# UI Visual Refresh — "Polished Plaster Studio"

Date: 2026-08-17
Status: Approved (direction + scope confirmed with user)

## Goal

The kiosk UI looks rough: flat surfaces, clashing light-mode inline colors on a dark
theme, no real typography, no depth, no motion. This refresh makes it look like a
well-made workshop sign — warm charcoal, cream, and brass — while changing **zero**
application logic.

Non-goals:

- No changes to Rust backend, Tauri commands, or data flow.
- No changes to event bindings, `data-*` attributes, or render function logic in
  `ui/js/app.js` / `ui/js/auth.js` (template strings and classes only).
- No external/CDN dependencies (kiosk is offline). All fonts and icons are local.

## Approach

Design tokens + restyle in place. Full rewrite of `ui/styles.css` as a design system
(tokens → components), light template tweaks in the JS render functions (inline SVG
icons, status chips, avatar initials, class names), and a bundled variable font.

## Design tokens

### Color

| Token | Value | Use |
|---|---|---|
| `--bg-0` | `#141210` | Page background (warm charcoal) |
| `--bg-1` | `#1c1917` | Panels, topbar, side nav |
| `--bg-2` | `#262220` | Raised surfaces: cards, inputs, hover states |
| `--bg-3` | `#322d29` | Active/pressed surfaces |
| `--bg-inset` | `#100e0c` | Inputs, log boxes (darker than panels) |
| `--text-1` | `#f2ead9` | Primary text (warm cream) |
| `--text-2` | `#b8ad9c` | Muted text |
| `--text-3` | `#7d7466` | Faint text, placeholders |
| `--line` | `#3a342e` | Borders |
| `--line-2` | `#4a423a` | Stronger borders, focus-adjacent |
| `--brass` | `#d9a441` | Primary accent: primary buttons, active nav, focus rings, highlights |
| `--brass-2` | `#e6b85c` | Brass hover |
| `--brass-ink` | `#1a1508` | Text on brass buttons |
| `--ok` | `#5aa469` | Success (sage) |
| `--warn` | `#e07b39` | Warning (caution orange — clearly distinct from brass gold) |
| `--err` | `#d95c5c` | Danger (clay) |

Tinted backgrounds for chips/status bars are derived with `color-mix()` or fixed
low-alpha values (e.g. `rgba(217,164,65,0.12)`), never light-mode colors.

### Surfaces & shape

- Page: `--bg-0` + two layers:
  1. Soft radial warm glow (brass at ~4% alpha) centered upper-middle.
  2. Plaster grain: inline SVG `feTurbulence` noise data-URI at ~3% opacity.
- Panels/cards: `--bg-1`, 1px `--line` border, radius 14px, shadow
  `0 8px 24px rgba(0,0,0,0.35)`.
- Inputs/log boxes: `--bg-inset`, radius 10px.
- Buttons: radius 10px; primary = brass bg + `--brass-ink` text, weight 700.
- Home Start button: pill (radius 999px), brass, glow shadow
  `0 0 40px rgba(217,164,65,0.25)`.
- Modals: radius 16px, shadow `0 24px 80px rgba(0,0,0,0.5)`.

### Typography

- Font: **Inter variable** (single woff2, weights 100–900), bundled at
  `ui/assets/fonts/inter-var.woff2`, loaded via `@font-face` in `styles.css`.
  Fallback stack: `Inter, ui-sans-serif, system-ui, "Segoe UI", sans-serif`.
- Clock: weight 800, `font-variant-numeric: tabular-nums` (prevents per-second
  jitter), cream with a soft text-shadow glow; the seconds group rendered in brass.
- Date line: weight 600, muted, letter-spaced small caps.
- Panel/section headings: weight 700.
- Small-caps letter-spaced labels for table headers and metric labels.
- Base size ~1rem; kiosk touch targets unchanged (buttons min-height 52px,
  side-nav items 58px).

## Screen treatments

### Home (clock screen)

- Logo mark (small, ~64px) above the clock.
- Clock time: cream, glow; seconds in brass. `tickClock` writes two spans
  (`HH:MM:` + `SS`) instead of one text node so the seconds can be brass; the
  `data-time`/`data-date` attributes and 1s interval are unchanged.
- Date: muted small caps below.
- Start button: large brass pill with glow + hover lift (`translateY(-2px)`).
- Background radial glow centered behind the clock.

### Role menu / staff picker

- Tiles: 14px radius, `--bg-1`, hover lift + border brightening.
- Each role tile gets an inline SVG icon (admin = shield, staff = person,
  brochure = book) above the label, muted → brass on hover.
- Staff tiles: initials avatar circle (first letters of first + last name, brass
  tint background, cream text) above the name; employee ID stays as the sub-label.

### Workspace (admin + staff)

- Topbar: slimmer; circular icon back button (44px); logo 44px; title weight 700.
- Session timer: proper chip — small clock icon + "Session 4:32", dot indicator;
  warn state = amber tint, danger state = clay tint + existing pulse animation.
  No light-mode backgrounds.
- Side nav: each item = icon + label; active = brass-tinted bg
  (`rgba(217,164,65,0.12)`) + 3px brass left bar + brass text; hover = `--bg-2`.
- Panel: rounded card; header with weight-700 title; body padding 20px.

### Tables

- Wrapped in a rounded container (radius 12px, border `--line`), overflow hidden.
- Sticky header: `--bg-2`, small-caps letter-spaced `--text-2`.
- Zebra: even rows `rgba(255,255,255,0.02)`.
- Row hover: `--bg-2` (clickable rows get pointer + hover).
- Review rows: clay left border (3px) + `rgba(217,92,92,0.08)` tint.
- Numeric columns: `tabular-nums`.

### Metrics

- Cards: `--bg-1`, brass top accent (3px), label small-caps muted, number weight 800
  ~2rem.

### Forms

- Inputs: `--bg-2` inset, `--line` border; focus = brass border +
  `0 0 0 3px rgba(217,164,65,0.25)` ring (replaces amber outline, same idea).
- Checkbox pills (`.check`): hover `--bg-2`; checked = brass-tinted bg + brass border.
- Buttons: primary brass, ghost = transparent + `--line` border + hover `--bg-2`,
  warning amber, danger clay. All with 120ms transitions and active press
  (`scale(0.98)`).

### Modals

- Backdrop: `rgba(0,0,0,0.72)` + `backdrop-filter: blur(6px)`.
- Modal: radius 16px, `--bg-1`, deep shadow.
- Auth modal: fingerprint icon (80px) inside a pulsing brass ring (two expanding
  rings, 2s loop); scan status = dark-mode status bar with left accent bar:
  info (brass), warn (amber), ok (sage), err (clay) — classes, not inline colors.
  Password fallback appears below with a divider.
- Clock action modal: two large buttons — Clock in (brass), Clock out (clay outline).

### Status tags (payroll, dispatch)

- `.tag-ok` / `.tag-warn` / `.tag-err`: small rounded chips, tinted bg + colored
  text, replacing inline `#5a5` / `#da5` / `#e55` spans.

## Motion

- Screen navigation: `.screen` fades in + slides up 8px, 150ms ease-out.
  (Works because each navigation replaces `innerHTML` — new element animates on
  insertion.)
- Panel body content: 120ms fade on `setPanel` updates.
- Buttons/tiles: 120ms transform + shadow transitions; active press `scale(0.98)`.
- Fingerprint ring pulse: 2s loop (replaces/refines existing `fp-radiance`).
- Status pill danger: keep existing pulse.
- All motion wrapped in `@media (prefers-reduced-motion: no-preference)`.

## Inline color cleanup (app.js / auth.js)

Every inline light-mode color is replaced with a CSS class:

| Location (approx) | Current | Replacement |
|---|---|---|
| `app.js` payroll/dispatch status spans | `style="color:#5a5/#da5/#e55"` | `class="tag tag-ok/warn/err"` |
| `app.js` enroll scan status | `style="background:#eaf7ff; color:#2c3e50"` etc. | `class="scan-status info/warn/ok/err"` |
| `app.js` cancel button | `style="color:#e74c3c..."` | `class="ghost danger-text"` |
| `auth.js` fp status updates | inline bg/color pairs | `class="scan-status info/warn/err"` |
| `styles.css` `.status-pill.warn/.danger` | Bootstrap light colors | dark-mode tints (see tokens) |
| `styles.css` `#cornice-search-results` | `color:#666` | `--text-3` |

`cellLooksHtml()` currently only passes through cells starting with `<button`.
Tag spans (`<span class="tag ...">`) must also pass through un-escaped. Change the
check to return true when the trimmed cell starts with `<button` **or**
`<span class="tag`. Nothing else. This is a template-rendering tweak, not logic.

## Icons

Inline SVG, stroke-based, 24px viewBox, `stroke="currentColor"`, `fill="none"`,
stroke-width 2, `stroke-linecap/linejoin="round"`. No icon library (offline).

Set (covers all 12 admin tabs, 3 role tiles, and common actions):
back-arrow, fingerprint, clock, user, users, shield, book, box, layers, truck,
gauge, database, bell, alert-triangle, dollar, list, check, x, plus, refresh.

Tab → icon map: alerts=bell, employees=users, enroll=fingerprint, payroll=dollar,
dispatch=truck, cornice_stock=box, mould_inventory=layers, stock=box, rates=gauge,
time=clock, logs=list, database=database.

Implementation: a small `ui/js/icons.js` module exporting
`icon(name, size?)` returning an SVG string; render functions call it inside
template literals.

## Files changed

| File | Change |
|---|---|
| `ui/styles.css` | Full rewrite as design system (tokens, components, motion) |
| `ui/js/icons.js` | New — inline SVG icon helper |
| `ui/js/app.js` | Template tweaks only: icons in nav/topbar/tiles, avatar initials, chips/tags/scan-status classes, status-pill markup, `cellLooksHtml` extension, `tickClock` two-span time |
| `ui/js/auth.js` | Template tweaks only: fingerprint ring markup, scan-status classes, remove inline colors |
| `ui/index.html` | Minor: title/meta unchanged; no structural change expected |
| `ui/assets/fonts/inter-var.woff2` | New — bundled variable font (~300KB) |
| `src-tauri/tauri.conf.json` | `backgroundColor` → `#141210` (matches new page bg, avoids load flash) |

## Verification

1. `ui/index.html` opened directly in a browser: home clock, role menu, staff
   picker render and animate (no Tauri needed for these screens).
2. `cd src-tauri && cargo check` passes (no Rust changes expected).
3. User runs the app (`cargo tauri dev` or release binary) to verify workspace,
   tables, modals, and enrollment/auth flows visually.
4. Grep confirms no remaining inline light-mode colors
   (`#eaf7ff`, `#2c3e50`, `#fff3e6`, `#f8d7da`, `#5a5`, `#da5`, `#e55`, `#666`)
   in `ui/`.
