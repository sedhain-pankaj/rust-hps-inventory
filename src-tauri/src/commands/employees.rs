use std::sync::{Arc, Mutex};

use sqlx::Row;
use tauri::State;

use crate::{
    db::{
        employee_by_id, hash_password, is_legacy_password_hash, list_employees, verify_password,
        AppState, FingerprintAuthJob, FingerprintEnrollJob,
    },
    fingerprint,
    models::*,
};

use super::{to_string, CommandResult};

#[tauri::command]
pub async fn list_staff(
    state: State<'_, AppState>,
    include_inactive: bool,
) -> CommandResult<Vec<Employee>> {
    list_employees(&state.db, include_inactive)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub async fn save_employee(
    state: State<'_, AppState>,
    input: EmployeeInput,
) -> CommandResult<Employee> {
    if input.id.trim().is_empty() {
        return Err("Employee ID is required.".to_string());
    }
    if input.name.trim().is_empty() {
        return Err("Employee name is required.".to_string());
    }

    if input.expect_new {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM employees WHERE id = ? COLLATE NOCASE)",
        )
        .bind(input.id.trim())
        .fetch_one(&state.db)
        .await
        .map_err(to_string)?;
        if exists {
            return Err("Employee ID already exists.".to_string());
        }
    }

    let now = crate::db::now_string();
    let password_hash = input
        .password
        .as_ref()
        .map(|password| password.trim())
        .filter(|password| !password.is_empty())
        .map(hash_password);

    sqlx::query(
        r#"
        INSERT INTO employees
            (id, name, finger, active, is_admin, password_hash, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            finger = excluded.finger,
            active = excluded.active,
            is_admin = excluded.is_admin,
            password_hash = COALESCE(excluded.password_hash, employees.password_hash),
            updated_at = excluded.updated_at
        "#,
    )
    .bind(input.id.trim())
    .bind(input.name.trim())
    .bind(input.finger.trim())
    .bind(input.active as i64)
    .bind(input.is_admin as i64)
    .bind(password_hash)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    // Update staff_category if provided and valid
    let category = input.staff_category.trim();
    if !category.is_empty()
        && ["cornice_hand", "storekeeper", "non_cornice", "driver", "helper"].contains(&category)
    {
        sqlx::query("UPDATE employees SET staff_category = ? WHERE id = ?")
            .bind(category)
            .bind(input.id.trim())
            .execute(&state.db)
            .await
            .map_err(to_string)?;
    }

    let mut permissions = input.permissions;
    if input.is_admin {
        for permission in [
            "clock",
            "cornice_log",
            "production_log",
            "overstock",
            "deliveries",
            "cornice_rates_view",
            "daily_production_all",
        ] {
            if !permissions.iter().any(|item| item == permission) {
                permissions.push(permission.to_string());
            }
        }
    }

    sqlx::query("DELETE FROM employee_permissions WHERE employee_id = ?")
        .bind(input.id.trim())
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    for permission in permissions {
        sqlx::query(
            "INSERT OR IGNORE INTO employee_permissions (employee_id, permission) VALUES (?, ?)",
        )
        .bind(input.id.trim())
        .bind(permission)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    }

    employee_by_id(&state.db, input.id.trim())
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Employee was saved but could not be reloaded.".to_string())
}

#[tauri::command]
pub async fn authenticate_password(
    state: State<'_, AppState>,
    employee_id: Option<String>,
    password: String,
    require_admin: bool,
) -> CommandResult<AuthResponse> {
    let submitted_password = password.trim().to_string();
    let candidates = if let Some(employee_id) = employee_id.filter(|id| !id.trim().is_empty()) {
        sqlx::query("SELECT id, password_hash, is_admin, active FROM employees WHERE id = ?")
            .bind(employee_id.trim())
            .fetch_all(&state.db)
            .await
            .map_err(to_string)?
    } else {
        sqlx::query("SELECT id, password_hash, is_admin, active FROM employees WHERE active = 1")
            .fetch_all(&state.db)
            .await
            .map_err(to_string)?
    };

    for row in candidates {
        let active = row.get::<i64, _>("active") != 0;
        let is_admin = row.get::<i64, _>("is_admin") != 0;
        let stored: Option<String> = row.get("password_hash");
        if !active || (require_admin && !is_admin) {
            continue;
        }
        if stored
            .as_deref()
            .map(|hash| verify_password(hash, &submitted_password))
            .unwrap_or(false)
        {
            let id: String = row.get("id");
            if stored
                .as_deref()
                .map(is_legacy_password_hash)
                .unwrap_or(false)
            {
                let upgraded = hash_password(&submitted_password);
                sqlx::query("UPDATE employees SET password_hash = ?, updated_at = ? WHERE id = ?")
                    .bind(upgraded)
                    .bind(crate::db::now_string())
                    .bind(&id)
                    .execute(&state.db)
                    .await
                    .map_err(to_string)?;
            }
            let employee = employee_by_id(&state.db, &id)
                .await
                .map_err(to_string)?
                .ok_or_else(|| "Employee no longer exists.".to_string())?;
            return Ok(AuthResponse {
                employee,
                source: "password".to_string(),
            });
        }
    }

    Err("Password was not accepted.".to_string())
}

#[tauri::command]
pub async fn start_fingerprint_auth(
    state: State<'_, AppState>,
    require_admin: bool,
    employee_id: Option<String>,
) -> CommandResult<FingerprintAuthStartResponse> {
    let job_id = state.next_auth_job_id();
    {
        let mut jobs = state
            .auth_jobs
            .lock()
            .map_err(|_| "Could not create authentication job.".to_string())?;
        jobs.insert(
            job_id.clone(),
            FingerprintAuthJob {
                matched_id: None,
                lines: vec!["Starting fingerprint identification…".to_string()],
                done: false,
                error: None,
            },
        );
    }

    let db = state.db.clone();
    let paths = state.paths.clone();
    let jobs = state.auth_jobs.clone();
    let active_pids = state.active_helper_pids.clone();
    let require_admin_clone = require_admin;
    let job_id_for_spawn = job_id.clone();
    // Match only the relevant gallery: the specific employee when the staff
    // modal triggered the scan, admins only for admin-gated scans.
    let template_filter = if let Some(id) = employee_id {
        fingerprint::TemplateFilter::Employee(id)
    } else if require_admin {
        fingerprint::TemplateFilter::Admins
    } else {
        fingerprint::TemplateFilter::All
    };

    tauri::async_runtime::spawn(async move {
        let job_id_for_task = job_id_for_spawn;

        // Pre-check: if job was cancelled before we start
        {
            let should_cancel = {
                if let Ok(all_jobs) = jobs.lock() {
                    all_jobs.get(&job_id_for_task).map(|j| j.done).unwrap_or(false)
                } else {
                    false
                }
            };
            if should_cancel {
                fingerprint::kill_orphaned_helpers(&active_pids);
                return;
            }
        }

        let job_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let progress_for_callback = Arc::clone(&job_lines);
        let progress = Arc::new(move |line: String| {
            if let Ok(mut lines) = progress_for_callback.lock() {
                lines.push(line);
            }
        });

        let result =
            fingerprint::identify_employee(&db, &paths, Some(progress), &active_pids, &template_filter)
                .await;

        // Resolve employee validation outside of job lock
        let resolution = match result {
            Ok(employee_id) => {
                match employee_by_id(&db, &employee_id).await {
                    Ok(Some(emp)) => {
                        if !emp.active {
                            Err(format!("{} is inactive.", emp.name))
                        } else if require_admin_clone && !emp.is_admin {
                            Err("This fingerprint does not have admin privilege.".to_string())
                        } else {
                            Ok(employee_id)
                        }
                    }
                    _ => Err("Fingerprint matched an unknown employee.".to_string()),
                }
            }
            Err(error) => Err(error.to_string()),
        };

        // Update job state
        if let Ok(mut all_jobs) = jobs.lock() {
            if let Some(job) = all_jobs.get_mut(&job_id_for_task) {
                if job.done {
                    return;
                }
                // Merge streamed lines into job
                if let Ok(lines) = job_lines.lock() {
                    for line in lines.iter() {
                        if !job.lines.contains(line) {
                            job.lines.push(line.clone());
                        }
                    }
                }
                match resolution {
                    Ok(mid) => {
                        job.matched_id = Some(mid);
                        job.done = true;
                    }
                    Err(err) => {
                        job.error = Some(err);
                        job.done = true;
                    }
                }
            }
        }
    });

    Ok(FingerprintAuthStartResponse { job_id })
}

#[tauri::command]
pub async fn poll_fingerprint_auth(
    state: State<'_, AppState>,
    job_id: String,
    from_index: Option<usize>,
) -> CommandResult<FingerprintAuthStatusResponse> {
    let start = from_index.unwrap_or(0);
    let (matched_id, done, error, next_index, lines) = {
        let jobs = state
            .auth_jobs
            .lock()
            .map_err(|_| "Could not read authentication job.".to_string())?;
        let job = jobs
            .get(&job_id)
            .ok_or_else(|| "Authentication job was not found.".to_string())?;
        let next_index = job.lines.len();
        let lines = if start < next_index {
            job.lines[start..].to_vec()
        } else {
            Vec::new()
        };
        (
            job.matched_id.clone(),
            job.done,
            job.error.clone(),
            next_index,
            lines,
        )
    };

    let employee = if done && error.is_none() {
        if let Some(ref mid) = matched_id {
            employee_by_id(&state.db, mid).await.map_err(to_string).ok().flatten()
        } else {
            None
        }
    } else {
        None
    };

    let state_name = if !done {
        "running"
    } else if error.is_some() {
        "failed"
    } else {
        "done"
    };

    Ok(FingerprintAuthStatusResponse {
        job_id,
        state: state_name.to_string(),
        lines,
        next_index,
        error,
        employee,
    })
}

#[tauri::command]
pub async fn cancel_fingerprint_auth(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<String> {
    fingerprint::kill_orphaned_helpers(&state.active_helper_pids);
    if let Ok(mut jobs) = state.auth_jobs.lock() {
        if let Some(job) = jobs.get_mut(&job_id) {
            if !job.done {
                job.error = Some("Authentication cancelled.".to_string());
                job.done = true;
            }
        }
    }
    Ok("cancelled".to_string())
}

#[tauri::command]
pub async fn start_fingerprint_enroll(
    state: State<'_, AppState>,
    employee_id: String,
    finger: String,
) -> CommandResult<FingerprintEnrollStartResponse> {
    let employee = employee_by_id(&state.db, &employee_id)
        .await
        .map_err(to_string)?
        .ok_or_else(|| "Choose a saved employee before enrolling a fingerprint.".to_string())?;
    let finger = if finger.trim().is_empty() {
        "right-index".to_string()
    } else {
        finger.trim().to_string()
    };
    let job_id = state.next_enroll_job_id();
    {
        let mut jobs = state
            .enroll_jobs
            .lock()
            .map_err(|_| "Could not create enrollment job.".to_string())?;
        jobs.insert(
            job_id.clone(),
            FingerprintEnrollJob {
                employee_id: employee.id.clone(),
                lines: vec!["Starting enrollment. Follow the reader prompts.".to_string()],
                done: false,
                error: None,
            },
        );
    }

    let db = state.db.clone();
    let paths = state.paths.clone();
    let jobs = state.enroll_jobs.clone();
    let active_pids = state.active_helper_pids.clone();
    let job_id_for_task = job_id.clone();
    let employee_id_for_task = employee.id.clone();
    tauri::async_runtime::spawn(async move {
        let progress_jobs = jobs.clone();
        let progress_job_id = job_id_for_task.clone();
        let progress = Arc::new(move |line: String| {
            if let Ok(mut all_jobs) = progress_jobs.lock() {
                if let Some(job) = all_jobs.get_mut(&progress_job_id) {
                    job.lines.push(line);
                }
            }
        });

        // Check cancellation before starting enrollment
        {
            let guard = jobs.lock().unwrap();
            if let Some(job) = guard.get(&job_id_for_task) {
                if job.done {
                    fingerprint::kill_orphaned_helpers(&active_pids);
                    return;
                }
            }
        }

        let result = fingerprint::enroll_employee(
            &db,
            &paths,
            &employee_id_for_task,
            &finger,
            Some(progress),
            &active_pids,
        )
        .await;

        // Only update job state if it hasn't been cancelled by another thread
        if let Ok(mut all_jobs) = jobs.lock() {
            if let Some(job) = all_jobs.get_mut(&job_id_for_task) {
                if job.done {
                    return;
                }
                match result {
                    Ok(messages) => {
                        if !messages.is_empty() {
                            job.lines = messages;
                        }
                        job.done = true;
                    }
                    Err(error) => {
                        job.error = Some(error.to_string());
                        job.done = true;
                    }
                }
            }
        }
    });

    Ok(FingerprintEnrollStartResponse { job_id })
}

#[tauri::command]
pub async fn poll_fingerprint_enroll(
    state: State<'_, AppState>,
    job_id: String,
    from_index: Option<usize>,
) -> CommandResult<FingerprintEnrollStatusResponse> {
    let start = from_index.unwrap_or(0);
    let (employee_id, done, error, next_index, lines) = {
        let jobs = state
            .enroll_jobs
            .lock()
            .map_err(|_| "Could not read enrollment job.".to_string())?;
        let job = jobs
            .get(&job_id)
            .ok_or_else(|| "Enrollment job was not found.".to_string())?;
        let next_index = job.lines.len();
        let lines = if start < next_index {
            job.lines[start..].to_vec()
        } else {
            Vec::new()
        };
        (
            job.employee_id.clone(),
            job.done,
            job.error.clone(),
            next_index,
            lines,
        )
    };

    let employee = if done && error.is_none() {
        employee_by_id(&state.db, &employee_id)
            .await
            .map_err(to_string)?
    } else {
        None
    };
    let state_name = if !done {
        "running"
    } else if error.is_some() {
        "failed"
    } else {
        "done"
    };
    Ok(FingerprintEnrollStatusResponse {
        job_id,
        state: state_name.to_string(),
        lines,
        next_index,
        error,
        employee,
    })
}

#[tauri::command]
pub fn cancel_fingerprint_enroll(
    state: State<'_, AppState>,
    job_id: String,
) -> CommandResult<()> {
    fingerprint::kill_orphaned_helpers(&state.active_helper_pids);
    if let Ok(mut jobs) = state.enroll_jobs.lock() {
        if let Some(job) = jobs.get_mut(&job_id) {
            if !job.done {
                job.lines.push("Enrollment cancelled by user.".to_string());
                job.error = Some("Cancelled".to_string());
                job.done = true;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn kill_fingerprint_helpers(state: State<'_, AppState>) -> CommandResult<()> {
    fingerprint::kill_orphaned_helpers(&state.active_helper_pids);
    Ok(())
}

#[tauri::command]
pub fn read_fingerprint_progress(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    let lines = state
        .fingerprint_progress
        .lock()
        .map_err(|_| "Could not read fingerprint progress.".to_string())?
        .clone();
    Ok(lines)
}

#[tauri::command]
pub fn clear_fingerprint_progress(state: State<'_, AppState>) -> CommandResult<()> {
    state
        .fingerprint_progress
        .lock()
        .map_err(|_| "Could not clear fingerprint progress.".to_string())?
        .clear();
    Ok(())
}

