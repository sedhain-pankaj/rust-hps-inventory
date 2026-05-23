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

The WA28/CS9711 helper source is bundled in `libfprint-CS9711/`. Build it with:

```bash
meson setup libfprint-CS9711/build libfprint-CS9711 -Ddrivers=cs9711 -Ddoc=false -Dgtk-examples=false -Dintrospection=false -Dinstalled-tests=false -Dudev_rules=disabled -Dudev_hwdb=disabled
ninja -C libfprint-CS9711/build examples/employee-clock-helper
```

The Rust app embeds the built helper and libfprint artifact when they exist at the expected build paths.
