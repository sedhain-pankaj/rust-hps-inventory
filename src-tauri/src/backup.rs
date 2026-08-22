use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, Timelike, Weekday};
use sqlx::SqlitePool;
use tauri::{AppHandle, State};

use crate::{
    db::{notification, AppPaths, AppState},
    models::{BackupInfo, BackupStatus, BackupTierStatus},
};

const WEEKLY_KEEP: usize = 4;
const MONTHLY_KEEP: usize = 12;
const TICK: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
pub enum BackupTier {
    Weekly,
    Monthly,
}

impl BackupTier {
    fn dir_name(&self) -> &'static str {
        match self {
            BackupTier::Weekly => "weekly",
            BackupTier::Monthly => "monthly",
        }
    }

    fn keep(&self) -> usize {
        match self {
            BackupTier::Weekly => WEEKLY_KEEP,
            BackupTier::Monthly => MONTHLY_KEEP,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            BackupTier::Weekly => "weekly",
            BackupTier::Monthly => "monthly",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            BackupTier::Weekly => "Weekly",
            BackupTier::Monthly => "Monthly",
        }
    }
}

fn backup_root(paths: &AppPaths) -> PathBuf {
    paths.source_root.join("data").join("backup")
}

fn tier_dir(paths: &AppPaths, tier: BackupTier) -> PathBuf {
    backup_root(paths).join(tier.dir_name())
}

fn list_backups(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("hps-") && name.ends_with(".db"))
        })
        .collect();
    files.sort();
    files
}

fn backup_info(path: &Path) -> Option<BackupInfo> {
    let meta = fs::metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let modified = meta
        .modified()
        .ok()
        .and_then(|mtime| mtime.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|age| {
            DateTime::from_timestamp(age.as_secs() as i64, 0).map(|dt| {
                dt.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
        })
        .unwrap_or_default();
    Some(BackupInfo {
        path: path.to_string_lossy().to_string(),
        name,
        modified,
        size_bytes: meta.len(),
    })
}

fn latest_mtime(dir: &Path) -> Option<SystemTime> {
    list_backups(dir)
        .last()
        .and_then(|path| fs::metadata(path).ok()?.modified().ok())
}

fn retain(paths: &AppPaths, tier: BackupTier) {
    let dir = tier_dir(paths, tier);
    let files = list_backups(&dir);
    let excess = files.len().saturating_sub(tier.keep());
    for old in files.iter().take(excess) {
        let _ = fs::remove_file(old);
    }
}

pub async fn backup_now(
    db: &SqlitePool,
    paths: &AppPaths,
    tier: BackupTier,
) -> Result<BackupInfo, String> {
    let dir = tier_dir(paths, tier);
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Could not create backup directory: {err}"))?;

    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let mut target = dir.join(format!("hps-{stamp}.db"));
    let mut suffix = 1;
    while target.exists() {
        target = dir.join(format!("hps-{stamp}-{suffix}.db"));
        suffix += 1;
    }

    let target_sql = target.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{target_sql}'"))
        .execute(db)
        .await
        .map_err(|err| format!("VACUUM INTO failed: {err}"))?;

    retain(paths, tier);

    backup_info(&target).ok_or_else(|| "Backup file missing after VACUUM INTO".to_string())
}

fn iso_week_start(now: DateTime<Local>) -> NaiveDateTime {
    let days_from_monday = now.weekday().num_days_from_monday() as i64;
    (now
        .date_naive()
        - chrono::Duration::days(days_from_monday))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
}

fn weekly_due(now: DateTime<Local>, latest: Option<SystemTime>) -> bool {
    if now.weekday() != Weekday::Sun || now.hour() < 12 {
        return false;
    }
    let week_start = iso_week_start(now)
        .and_local_timezone(Local)
        .earliest()
        .expect("local midnight is always resolvable")
        .timestamp();
    match latest {
        Some(mtime) => mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|age| (age.as_secs() as i64) < week_start)
            .unwrap_or(true),
        None => true,
    }
}

fn monthly_due(now: DateTime<Local>, latest: Option<SystemTime>) -> bool {
    if now.weekday() != Weekday::Sun || now.hour() < 12 || now.day() > 7 {
        return false;
    }
    let month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .expect("day 1 is always valid")
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_local_timezone(Local)
        .earliest()
        .expect("local midnight is always resolvable")
        .timestamp();
    match latest {
        Some(mtime) => mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|age| (age.as_secs() as i64) < month_start)
            .unwrap_or(true),
        None => true,
    }
}

async fn tick(db: &SqlitePool, paths: &AppPaths) {
    let now = Local::now();
    let force = std::env::var("HPS_BACKUP_FORCE").unwrap_or_default();
    for tier in [BackupTier::Weekly, BackupTier::Monthly] {
        let forced = force == tier.label() || force == "both";
        let due = forced
            || match tier {
                BackupTier::Weekly => {
                    weekly_due(now, latest_mtime(&tier_dir(paths, BackupTier::Weekly)))
                }
                BackupTier::Monthly => {
                    monthly_due(now, latest_mtime(&tier_dir(paths, BackupTier::Monthly)))
                }
            };
        if !due {
            continue;
        }
        match backup_now(db, paths, tier).await {
            Ok(info) => {
                eprintln!("[backup] {} backup written: {}", tier.label(), info.path)
            }
            Err(err) => {
                eprintln!("[backup] {} backup failed: {err}", tier.label());
                let message = format!("{} database backup failed: {err}", tier.title());
                if let Err(alert_err) =
                    notification(db, "yellow", "backup", &message, "", None).await
                {
                    eprintln!("[backup] could not record failure alert: {alert_err}");
                }
            }
        }
    }
}

pub fn spawn_scheduler(db: SqlitePool, paths: AppPaths) {
    std::thread::Builder::new()
        .name("backup-scheduler".into())
        .spawn(move || {
            loop {
                tauri::async_runtime::block_on(tick(&db, &paths));
                std::thread::sleep(TICK);
            }
        })
        .expect("could not spawn backup scheduler thread");
}

#[tauri::command]
pub async fn create_database_backup(state: State<'_, AppState>) -> Result<BackupInfo, String> {
    backup_now(&state.db, &state.paths, BackupTier::Weekly).await
}

fn tier_status(paths: &AppPaths, tier: BackupTier) -> BackupTierStatus {
    let files = list_backups(&tier_dir(paths, tier));
    BackupTierStatus {
        count: files.len(),
        latest: files.last().and_then(|path| backup_info(path)),
    }
}

#[tauri::command]
pub async fn backup_status(state: State<'_, AppState>) -> Result<BackupStatus, String> {
    Ok(BackupStatus {
        weekly: tier_status(&state.paths, BackupTier::Weekly),
        monthly: tier_status(&state.paths, BackupTier::Monthly),
    })
}

#[tauri::command]
pub fn exit_kiosk(app: AppHandle, state: State<'_, AppState>) {
    state.allow_exit.store(true, Ordering::Relaxed);
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("hps-backup-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn cleanup(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    fn test_paths(root: &Path) -> AppPaths {
        AppPaths {
            data_dir: root.join("data"),
            db_path: root.join("hps.db"),
            fingerprint_dir: root.join("data").join("fingerprints"),
            resource_dir: None,
            source_root: root.to_path_buf(),
        }
    }

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
            .and_local_timezone(Local)
            .single()
            .unwrap()
    }

    fn mtime_at(secs: i64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
    }

    #[test]
    fn weekly_due_only_sunday_noon_without_current_week_backup() {
        // 2026-08-02 is a Sunday, 2026-08-01 a Saturday
        let sunday_noon = at(2026, 8, 2, 12, 0);
        assert!(weekly_due(sunday_noon, None));
        assert!(!weekly_due(at(2026, 8, 1, 12, 0), None));
        assert!(!weekly_due(at(2026, 8, 2, 11, 59), None));
        assert!(weekly_due(at(2026, 8, 2, 23, 30), None));

        let week_start = iso_week_start(sunday_noon)
            .and_local_timezone(Local)
            .earliest()
            .unwrap()
            .timestamp();
        // Backup from this week -> not due
        assert!(!weekly_due(sunday_noon, Some(mtime_at(week_start + 3600))));
        // Backup from the previous week -> due
        assert!(weekly_due(sunday_noon, Some(mtime_at(week_start - 3600))));
    }

    #[test]
    fn monthly_due_only_first_sunday_noon_without_current_month_backup() {
        // 2026-08-02 is the first Sunday of August 2026
        let first_sunday = at(2026, 8, 2, 12, 0);
        assert!(monthly_due(first_sunday, None));
        // Second Sunday (day > 7) -> never due
        assert!(!monthly_due(at(2026, 8, 9, 12, 0), None));
        assert!(!monthly_due(at(2026, 8, 16, 12, 0), None));
        assert!(!monthly_due(at(2026, 8, 2, 11, 0), None));

        let month_start = at(2026, 8, 1, 0, 0).timestamp();
        assert!(!monthly_due(first_sunday, Some(mtime_at(month_start + 3600))));
        assert!(monthly_due(first_sunday, Some(mtime_at(month_start - 3600))));
    }

    #[test]
    fn retain_keeps_newest_per_tier_and_ignores_other_files() {
        let root = temp_root("retain");
        let paths = test_paths(&root);
        let wdir = tier_dir(&paths, BackupTier::Weekly);
        fs::create_dir_all(&wdir).unwrap();
        for i in 1..=6 {
            fs::write(wdir.join(format!("hps-2026010{i}-120000.db")), b"x").unwrap();
        }
        fs::write(wdir.join("notes.txt"), b"x").unwrap();

        retain(&paths, BackupTier::Weekly);

        let remaining = list_backups(&wdir);
        assert_eq!(remaining.len(), 4);
        assert!(remaining[0].ends_with("hps-20260103-120000.db"));
        assert!(remaining[3].ends_with("hps-20260106-120000.db"));
        assert!(wdir.join("notes.txt").exists());
        cleanup(&root);
    }

    async fn connect_pool(path: &Path) -> sqlx::SqlitePool {
        use sqlx::sqlite::SqliteConnectOptions;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn backup_now_creates_consistent_copy_with_data() {
        let root = temp_root("vacuum");
        let paths = test_paths(&root);
        let db = connect_pool(&paths.db_path).await;
        sqlx::query("CREATE TABLE t (x TEXT)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t VALUES ('hello')")
            .execute(&db)
            .await
            .unwrap();

        let info = backup_now(&db, &paths, BackupTier::Weekly).await.unwrap();
        assert!(info.name.starts_with("hps-"));
        assert!(info.name.ends_with(".db"));
        assert!(info.size_bytes > 0);
        assert!(info.path.contains("weekly"));

        let check_options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&info.path)
            .read_only(true);
        let check = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(check_options)
            .await
            .unwrap();
        let integrity: String =
            sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&check).await.unwrap();
        assert_eq!(integrity, "ok");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM t")
            .fetch_one(&check)
            .await
            .unwrap();
        assert_eq!(count, 1);
        cleanup(&root);
    }

    #[tokio::test]
    async fn backup_now_applies_weekly_retention() {
        let root = temp_root("retention");
        let paths = test_paths(&root);
        let db = connect_pool(&paths.db_path).await;
        sqlx::query("CREATE TABLE t (x TEXT)")
            .execute(&db)
            .await
            .unwrap();

        for i in 0..5 {
            backup_now(&db, &paths, BackupTier::Weekly).await.unwrap();
            // Distinct filenames: bump the clock by touching nothing, rely on
            // the built-in same-second suffix; force distinctness manually.
            if i < 4 {
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        let files = list_backups(&tier_dir(&paths, BackupTier::Weekly));
        assert_eq!(files.len(), 4);
        cleanup(&root);
    }
}
