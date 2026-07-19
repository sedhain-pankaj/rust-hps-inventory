# Hopkins Inventory Management - AI Agent Reference

## Project Overview

**Hopkins Plaster Studio** inventory management and staff kiosk application. Two versions exist side-by-side:

| Version | Location | Status | Notes |
|---------|----------|--------|-------|
| Python (original) | `Hopkins-Inventory-Management-main/` | Legacy/reference | Working, streamlit-based UI, CSV storage |
| Rust/Tauri (current) | `src-tauri/`, `ui/` | Active development | Native desktop app, SQLite storage |

Both versions share the same C helper binary (`employee-clock-helper`) for fingerprint operations.

## Directory Structure

```
rust-hps-inventory/
├── src-tauri/                    # Rust/Tauri backend (active)
│   ├── src/
│   │   ├── commands.rs           # Tauri command handlers
│   │   ├── db.rs                 # SQLite database, AppPaths, AppState
│   │   ├── fingerprint.rs        # CRITICAL: Fingerprint helper spawning
│   │   └── main.rs               # App entry point
│   └── Cargo.toml
├── ui/                           # Frontend (HTML/CSS/JS)
│   ├── js/app.js                 # Main frontend logic
│   └── js/api.js                 # Tauri invoke/listen wrappers
├── Hopkins-Inventory-Management-main/  # Python legacy (reference)
│   ├── Utilities/fingerprint_service.py  # Reference for subprocess spawning
│   ├── Utilities/constants.py            # Shared constants
│   ├── admin_enroll_employee.py          # Working enrollment script
│   └── index.py                          # Streamlit app
├── libfprint-CS9711/             # CS9711 fingerprint driver + helper binary
│   ├── build/examples/employee-clock-helper  # C helper binary
│   ├── build/libfprint/libfprint-2.so.2.*    # Bundled libfprint library
│   └── libfprint/drivers/cs9711/cs9711.c     # Driver source
├── data/                         # Runtime data (fingerprints, employees)
├── hps.db                        # SQLite database (Rust version)
└── db_init.sql                   # Database schema
```

## Fingerprint Hardware: WA28 Reader with CS9711 Chipset

### Key Facts
- **Hardware**: Chipsailing WA28 fingerprint reader (USB device)
- **Chipset**: CS9711 (USB VID/PID: `2541:0236`, also `2541:9711`)
- **NOT a system driver**: The CS9711 driver is NOT installed on the OS. It's bundled WITHIN the app as `libfprint-2.so.2` and loaded via `LD_LIBRARY_PATH` at runtime.
- **Enrollment stages**: 15 finger scans required per enrollment
- **Protocol**: libfprint v2 API (version 1.94.10)

### Why No System Installation?
The fingerprint reader is app-specific - employees enroll ONLY for this kiosk, NOT for OS login. The app deliberately avoids fprintd/PAM integration. All templates are stored locally in `data/fingerprints/*.fpdata` and in the SQLite `fingerprint_templates` table.

### How Fingerprint Works (Architecture)
```
Tauri App (Rust) → spawns → employee-clock-helper (C binary)
                                        ↓
                              uses bundled libfprint-2.so.2
                                        ↓
                              communicates via libusb → USB device 2541:0236
```

The C helper is a standalone binary that:
1. Creates `FpContext`, finds CS9711 device
2. Opens device via `fp_device_open_sync()`
3. For enrollment: calls `fp_device_enroll_sync()` (15 stages)
4. For identification: calls `fp_device_identify_sync()` against gallery
5. Outputs structured lines to stdout: `DEVICE|...`, `PROGRESS|...`, `ERROR|...`, etc.

### Helper Binary Location Resolution (in order)
1. `HPS_FINGERPRINT_HELPER` env var
2. Extracted bundled binary: `~/.local/share/com.hopkinsplaster.kiosk/libfprint/employee-clock-helper`
3. Build directory: `libfprint-CS9711/build/examples/employee-clock-helper`

The helper is bundled via `include_bytes!()` in `fingerprint.rs` (lines 16-19).

## CRITICAL: Fingerprint Subprocess Spawning

### The Problem
The C helper uses libfprint which internally uses GLib's main loop and gusb for USB communication. When spawned from a Tauri app, several issues arise:

1. **Blocking I/O**: `read()` on pipe blocks when no data available (helper waits for finger scans)
2. **stderr flooding**: C helper sets `G_MESSAGES_DEBUG=all` which floods stderr with GLib debug output → pipe buffer deadlock
3. **sudo environment stripping**: Running with `sudo` clears `DBUS_SESSION_BUS_ADDRESS` → libfprint's gusb can't communicate properly
4. **Tauri IPC backpressure**: If event emission blocks, the reader thread deadlocks

### Current Solution (fingerprint.rs:222-351)
The `run_helper_with_events()` function handles all this:

```rust
// Key techniques:
1. Set O_NONBLOCK on stdout pipe via fcntl → read() returns WouldBlock instead of hanging
2. Byte-by-byte reading → processes output immediately, no line-buffering deadlock
3. Drop stderr (don't read it) → prevents stderr pipe buffer from filling and blocking child
4. Preserve DBUS_SESSION_BUS_ADDRESS → pass through to child process
5. Set LD_LIBRARY_PATH → child can find bundled libfprint-2.so.2
6. Set current_dir to helper's parent directory → matches Python's cwd=BASE_DIR
7. Loop checks: timeout (360s default), child.try_wait(), WouldBlock handling
8. Sleep 10ms per iteration → yields CPU for Tauri IPC event processing
```

### Why sudo Works (Eventually)
When run with `sudo`:
- USB device `/dev/bus/usb/001/004` has permissions `crw-rw-r-- root root`
- Root has write access, so libusb can open the device
- But sudo strips `DBUS_SESSION_BUS_ADDRESS`, which the code now preserves explicitly

### Why Non-sudo Needs udev Rule
Without sudo, user lacks write access to `/dev/bus/usb/XXX/YYY`. Need:
```
SUBSYSTEM=="usb", ATTR{idVendor}=="2541", ATTR{idProduct}=="0236", MODE="0660", GROUP="plugdev"
```

## Database Schema (SQLite)

Key tables in `hps.db`:
- **employees**: Staff registry with `id`, `name`, `finger`, `active`, `is_admin`, `password_hash`
- **fingerprint_templates**: Binary fingerprint data per employee (`template BLOB`)
- **time_clock_events**: Clock in/out records with `action` (clock_in/clock_out), `source` (fingerprint/password)
- **cornice_rates**: Product pricing (series, model, unit_text, unit_value)
- **stock_items**: Inventory tracking
- **cornice_logs**: Production logging per employee
- **admin_notifications**: System alerts

## Python Legacy as Reference

The Python version (`Hopkins-Inventory-Management-main/`) is the working reference for:
- How to properly spawn the helper subprocess (see `Utilities/fingerprint_service.py:_run_helper()`)
- CSV-based data storage format (employees.csv, time_clock_log.csv)
- Enrollment and identification flow

Key Python `_run_helper()` patterns that Rust should mimic:
```python
process = subprocess.Popen(
    command,
    cwd=BASE_DIR,              # Working directory matters
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,  # Merge stderr into stdout
    text=True,
    bufsize=1,                 # Line-buffered
)
# Uses selectors with timeout for non-blocking reads
```

## Build and Run

### Development
```bash
cd src-tauri
cargo run                    # Debug build
cargo build --release        # Release build
```

### Helper Binary Rebuild (if driver changes)
```bash
cd libfprint-CS9711
meson setup build . -Ddrivers=cs9711 -Ddoc=false -Dgtk-examples=false \
  -Dintrospection=false -Dinstalled-tests=false \
  -Dudev_rules=disabled -Dudev_hwdb=disabled
ninja -C build examples/employee-clock-helper
```

### Testing Fingerprint Directly
```bash
# With sudo (always works for USB access):
sudo LD_LIBRARY_PATH=libfprint-CS9711/build/libfprint \
  libfprint-CS9711/build/examples/employee-clock-helper enroll /tmp/fp-test EMP001 right-index

# Without sudo (needs udev rule):
LD_LIBRARY_PATH=libfprint-CS9711/build/libfprint \
  libfprint-CS9711/build/examples/employee-clock-helper enroll /tmp/fp-test EMP001 right-index
```

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `HPS_FINGERPRINT_HELPER` | Override helper binary path | Auto-detected |
| `HPS_FINGERPRINT_TIMEOUT` | Helper timeout in seconds | 360 |
| `HPS_FINGERPRINT_ATTEMPTS` | Identification retry count | 5 |

## Common Issues and Solutions

### Enrollment stuck on "Connecting to fingerprint scanner..."
- **Cause**: Helper process hangs, Rust reader blocks on pipe
- **Fix**: Current code uses O_NONBLOCK + timeout. Check if helper works standalone with sudo first.

### "Access denied (insufficient permissions) [-3]"
- **Cause**: USB device `/dev/bus/usb/XXX/YYY` not writable by current user
- **Fix**: Run with `sudo` or add udev rule for device 2541:0236

### Helper not found
- **Cause**: Build directory doesn't exist, bundled extraction failed
- **Fix**: Rebuild helper binary or set `HPS_FINGERPRINT_HELPER` env var

### libfprint library not found
- **Cause**: `LD_LIBRARY_PATH` not set to helper's directory
- **Fix**: Code sets this automatically. Verify `libfprint-2.so.2` exists alongside helper binary.

## Distribution Notes

For end users:
1. App bundles the helper binary AND libfprint-2.so.2 internally via `include_bytes!()`
2. On first run, extracts to `~/.local/share/com.hopkinsplaster.kiosk/libfprint/`
3. User runs with `sudo` for USB device access (simplest approach)
4. Alternatively: install udev rule + run without sudo

## Frontend Communication

Enrollment flow (ui/js/app.js):
1. User clicks "Start Enrollment" → shows "Connecting to fingerprint scanner..."
2. Listens for `fingerprint_progress` events from backend
3. Calls `invoke("enroll_fingerprint", {employeeId, finger})`
4. Backend spawns helper, streams output lines as events
5. On success: stores template in DB, returns employee data
6. On error: displays user-friendly message

Event payload format (from C helper):
- `DEVICE|name|driver|id` - Device detected
- `ENROLL_STAGES|N` - Number of required scans
- `READY|enroll` - Ready for finger placement
- `PROGRESS|completed|total` - Scan progress
- `RETRY|message` - Scan quality issue, retry needed
- `ERROR|message` - Fatal error
- `ENROLLED|employee_id|path` - Success

## Password Hash
Admin password hash (SHA-256): `74327943f791e17b6081b590be47d518d885b79972d37087df480448e0672094`
(Defined in both Python constants.py and Rust db.rs as LEGACY_ADMIN_HASH)
