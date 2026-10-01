# Rust HPS Inventory

Rust/Tauri v2 kiosk app for Hopkins Plaster Studio inventory, staff clocking, admin tools, and the shared SQLite database.

## Run and Build

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path src-tauri/Cargo.toml
```

The Tauri frontend is bundled from `ui/`. Seed data is compiled from `assets/cornice_rate.csv` and `assets/overall_stock.csv`.

## Database

The app stores its development/runtime SQLite database at `hps.db` in this folder. The schema lives in both Rust migrations in `src-tauri/src/db.rs` and the standalone `db_init.sql` reference file.

`hps.db`, `hps.db-shm`, and `hps.db-wal` are intentionally ignored.

## Fingerprint Helper

The URU4500 helper source is bundled in `libfprint-uru4500/` (vendored libfprint tree). Build it with:

```bash
meson setup libfprint-uru4500/build libfprint-uru4500 -Ddrivers=uru4000 -Ddoc=false -Dgtk-examples=false -Dintrospection=false -Dinstalled-tests=false -Dudev_rules=disabled -Dudev_hwdb=disabled
ninja -C libfprint-uru4500/build examples/employee-clock-helper
```

The Rust app embeds the built helper and libfprint artifact when they exist at the expected build paths.

### Alignment hints

Enrollment stores the 5 sub-print images as an `<employee_id>.fpimg` bundle alongside the
`.fpdata` template (both persisted to the `images`/`template` columns of `fingerprint_templates`).
When an identify scan fails, the helper emits:

- `BEST|<employee_id>|<score>` — closest employee and max Bozorth3 score (match threshold is 40)
- `HINT|<employee_id>|<ncc_pct>|<slide_x_mm>|<slide_y_mm>` — NCC alignment of the scanned finger
  against the enrolled sub-prints; signed slide in mm (+x right, +y down)

The auth modal shows `Match: <score>/40 for <name>` with a "shift your finger Xmm LEFT/RIGHT,
Ymm UP/DOWN" prompt, which stays visible across auto-rescans. Templates are exported per context:
the staff modal matches only the selected employee, admin-gated scans match only admin
fingerprint templates. Employees enrolled before the hint feature have no `.fpimg` bundle and
must re-enroll to get alignment hints.
