use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
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
        paths.data_dir.join("libfprint").join("employee-clock-helper"),
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
    let lines = tokio::task::spawn_blocking(move || {
        run_helper(&helper, &["identify".to_string(), path_string(&storage)])
    })
    .await
    .context("Fingerprint task failed")??;

    for line in lines {
        if let Some(employee_id) = line.strip_prefix("MATCH|") {
            return Ok(employee_id.trim().to_string());
        }
        if line == "NO_MATCH" {
            return Err(anyhow!("Fingerprint scanned, but no enrolled employee matched."));
        }
    }

    Err(anyhow!("Fingerprint helper finished without a match result."))
}

pub async fn enroll_employee(
    db: &sqlx::SqlitePool,
    paths: &AppPaths,
    employee_id: &str,
    finger: &str,
) -> Result<()> {
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
    tokio::task::spawn_blocking(move || run_helper(&helper_for_task, &args))
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

    Ok(())
}

async fn export_templates(db: &sqlx::SqlitePool, paths: &AppPaths) -> Result<()> {
    fs::create_dir_all(&paths.fingerprint_dir).context("Could not create fingerprint storage")?;
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

fn run_helper(helper: &Path, args: &[String]) -> Result<Vec<String>> {
    let mut command = Command::new(helper);
    command.args(args);
    if let Some(helper_dir) = helper.parent() {
        let existing = env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let value = if existing.is_empty() {
            helper_dir.to_string_lossy().to_string()
        } else {
            format!("{}:{existing}", helper_dir.to_string_lossy())
        };
        command.env("LD_LIBRARY_PATH", value);
    }

    let output = command
        .output()
        .with_context(|| format!("Could not run fingerprint helper at {}", helper.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<String> = stdout
        .lines()
        .chain(stderr.lines())
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if output.status.success() {
        return Ok(lines);
    }

    let message = lines
        .iter()
        .find_map(|line| line.strip_prefix("ERROR|").map(str::to_string))
        .or_else(|| lines.last().cloned())
        .unwrap_or_else(|| format!("Fingerprint helper exited with {}", output.status));
    Err(anyhow!(message))
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
