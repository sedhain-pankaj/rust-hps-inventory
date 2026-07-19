use std::{
    env, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use sqlx::Row;

use crate::db::{now_string, AppPaths};

const HELPER_ENV: &str = "HPS_FINGERPRINT_HELPER";
const BUNDLED_HELPER: &[u8] =
    include_bytes!("../../libfprint-CS9711/build/examples/employee-clock-helper");
const BUNDLED_LIBFPRINT: &[u8] =
    include_bytes!("../../libfprint-CS9711/build/libfprint/libfprint-2.so.2.0.0");

pub fn find_helper_binary(paths: &AppPaths) -> Option<PathBuf> {
    if let Ok(configured) = env::var(HELPER_ENV) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Ok(path) = extract_bundled_helper(paths) {
        if path.is_file() {
            return Some(path);
        }
    }

    let mut candidates = vec![
        paths.data_dir.join("employee-clock-helper"),
        paths
            .data_dir
            .join("libfprint")
            .join("employee-clock-helper"),
        paths
            .source_root
            .join("libfprint-CS9711")
            .join("build")
            .join("examples")
            .join("employee-clock-helper"),
        paths
            .source_root
            .join("libfprint-CS9711")
            .join("builddir")
            .join("examples")
            .join("employee-clock-helper"),
    ];

    if let Some(resource_dir) = &paths.resource_dir {
        candidates.push(resource_dir.join("employee-clock-helper"));
        candidates.push(resource_dir.join("libfprint").join("employee-clock-helper"));
    }

    candidates.into_iter().find(|path| path.is_file())
}

pub async fn identify_employee(db: &sqlx::SqlitePool, paths: &AppPaths) -> Result<String> {
    export_templates(db, paths).await?;
    let helper = find_helper_binary(paths).ok_or_else(helper_missing_error)?;
    let storage = paths.fingerprint_dir.clone();

    let attempts = env::var("HPS_FINGERPRINT_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5);
    let mut last_error = None;

    for attempt in 1..=attempts {
        let helper_for_task = helper.clone();
        let storage_for_task = storage.clone();
        let source_root_for_task = paths.source_root.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_helper(
                &helper_for_task,
                &["identify".to_string(), path_string(&storage_for_task)],
                &source_root_for_task,
            )
        })
        .await
        .context("Fingerprint task failed")?;

        match result {
            Ok(lines) => {
                for line in lines {
                    if let Some(employee_id) = line.strip_prefix("MATCH|") {
                        clear_template_cache(&storage);
                        return Ok(employee_id.trim().to_string());
                    }
                    if line == "NO_MATCH" {
                        last_error = Some(
                            "Fingerprint scanned, but no enrolled employee matched.".to_string(),
                        );
                    }
                }
            }
            Err(error) => {
                let message = error.to_string();
                if is_fatal_identify_error(&message) {
                    clear_template_cache(&storage);
                    return Err(anyhow!(message));
                }
                last_error = Some(message);
            }
        }

        if attempt < attempts {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }

    clear_template_cache(&storage);
    Err(anyhow!(
        "{} after {attempts} scan attempts. Use password fallback or re-enroll the fingerprint.",
        last_error
            .unwrap_or_else(|| "Fingerprint helper finished without a match result".to_string())
    ))
}

pub async fn enroll_employee(
    db: &sqlx::SqlitePool,
    paths: &AppPaths,
    employee_id: &str,
    finger: &str,
    on_line: Option<Arc<dyn Fn(String) + Send + Sync>>,
) -> Result<Vec<String>> {
    let helper = find_helper_binary(paths).ok_or_else(helper_missing_error)?;
    let storage = paths.fingerprint_dir.clone();
    fs::create_dir_all(&storage).context("Could not create fingerprint storage")?;

    let args = vec![
        "enroll".to_string(),
        path_string(&storage),
        employee_id.to_string(),
        finger.to_string(),
    ];
    let helper_for_task = helper.clone();
    let source_root_for_task = paths.source_root.clone();
    let lines = tokio::task::spawn_blocking(move || {
        run_helper_with_events(
            &helper_for_task,
            &args,
            on_line.as_deref(),
            &source_root_for_task,
        )
    })
    .await
    .context("Fingerprint enrollment task failed")??;

    let template_path = storage.join(format!("{employee_id}.fpdata"));
    let template = fs::read(&template_path).with_context(|| {
        format!(
            "Enrollment completed, but the fingerprint template was not readable at {}",
            template_path.display()
        )
    })?;

    sqlx::query(
        r#"
        INSERT INTO fingerprint_templates (employee_id, finger, template, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(employee_id) DO UPDATE SET
            finger = excluded.finger,
            template = excluded.template,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(employee_id)
    .bind(finger)
    .bind(template)
    .bind(now_string())
    .execute(db)
    .await?;

    let _ = fs::remove_file(template_path);

    Ok(lines)
}

async fn export_templates(db: &sqlx::SqlitePool, paths: &AppPaths) -> Result<()> {
    fs::create_dir_all(&paths.fingerprint_dir).context("Could not create fingerprint storage")?;
    clear_template_cache(&paths.fingerprint_dir);
    let rows = sqlx::query("SELECT employee_id, template FROM fingerprint_templates")
        .fetch_all(db)
        .await?;

    for row in rows {
        let employee_id: String = row.get("employee_id");
        let template: Vec<u8> = row.get("template");
        let path = paths.fingerprint_dir.join(format!("{employee_id}.fpdata"));
        fs::write(&path, template)?;
        set_private_permissions(&path);
    }

    Ok(())
}

fn clear_template_cache(storage: &Path) {
    let Ok(entries) = fs::read_dir(storage) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("fpdata") {
            let _ = fs::remove_file(path);
        }
    }
}

fn is_fatal_identify_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no fingerprint reader")
        || lower.contains("no compatible enrolled")
        || lower.contains("helper was not found")
        || lower.contains("permission")
        || lower.contains("open")
}

fn run_helper(helper: &Path, args: &[String], working_dir: &Path) -> Result<Vec<String>> {
    run_helper_with_events(helper, args, None, working_dir)
}

fn run_helper_with_events(
    helper: &Path,
    args: &[String],
    on_line: Option<&(dyn Fn(String) + Send + Sync)>,
    working_dir: &Path,
) -> Result<Vec<String>> {
    let mut command = Command::new(helper);
    command.args(args);
    command.current_dir(working_dir);
    if let Some(helper_dir) = helper.parent() {
        let existing = env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let value = if existing.is_empty() {
            helper_dir.to_string_lossy().to_string()
        } else {
            format!("{}:{existing}", helper_dir.to_string_lossy())
        };
        command.env("LD_LIBRARY_PATH", value);
    }
    command.env("G_MESSAGES_DEBUG", "");

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("Could not run fingerprint helper at {}", helper.display()))?;

    let (tx, rx) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        spawn_line_reader(stdout, tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_line_reader(stderr, tx.clone());
    }
    drop(tx);

    let timeout_secs = helper_timeout_seconds();
    let started = Instant::now();
    let mut lines = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(on_line) = on_line {
                    on_line(line.clone());
                }
                lines.push(line);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }

        while let Ok(line) = rx.try_recv() {
            if let Some(on_line) = on_line {
                on_line(line.clone());
            }
            lines.push(line);
        }

        if let Some(status) = child.try_wait().with_context(|| {
            format!(
                "Could not wait for fingerprint helper at {}",
                helper.display()
            )
        })? {
            if status.success() {
                return Ok(lines);
            }

            let message = lines
                .iter()
                .find_map(|line| line.strip_prefix("ERROR|").map(str::to_string))
                .or_else(|| lines.last().cloned())
                .unwrap_or_else(|| format!("Fingerprint helper exited with {status}"));
            return Err(anyhow!(message));
        }

        if timeout_secs > 0 && started.elapsed() > Duration::from_secs(timeout_secs) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "Fingerprint scan timed out after {timeout_secs} seconds."
            ));
        }
    }
}

fn spawn_line_reader<R>(reader: R, tx: mpsc::Sender<String>)
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() && is_protocol_line(&trimmed) {
                let _ = tx.send(trimmed);
            }
        }
    });
}

fn is_protocol_line(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "DEVICE|",
        "ENROLL_STAGES|",
        "READY|",
        "PROGRESS|",
        "RETRY|",
        "ERROR|",
        "ENROLLED|",
        "MATCH|",
        "NO_MATCH",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

fn helper_timeout_seconds() -> u64 {
    env::var("HPS_FINGERPRINT_TIMEOUT")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(360)
}

fn helper_missing_error() -> anyhow::Error {
    anyhow!(
        "Fingerprint helper was not found. Build or bundle libfprint-CS9711/examples/employee-clock-helper, or set {HELPER_ENV}."
    )
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn extract_bundled_helper(paths: &AppPaths) -> Result<PathBuf> {
    let dir = paths.data_dir.join("libfprint");
    fs::create_dir_all(&dir)?;

    let helper_path = dir.join("employee-clock-helper");
    write_if_different(&helper_path, BUNDLED_HELPER)?;
    set_executable_permissions(&helper_path);

    let versioned_lib = dir.join("libfprint-2.so.2.0.0");
    let soname_lib = dir.join("libfprint-2.so.2");
    write_if_different(&versioned_lib, BUNDLED_LIBFPRINT)?;
    write_if_different(&soname_lib, BUNDLED_LIBFPRINT)?;

    Ok(helper_path)
}

fn write_if_different(path: &Path, bytes: &[u8]) -> Result<()> {
    let needs_write = fs::metadata(path)
        .map(|metadata| metadata.len() != bytes.len() as u64)
        .unwrap_or(true);
    if needs_write {
        fs::write(path, bytes)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) {}

#[cfg(unix)]
fn set_executable_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path) {}
