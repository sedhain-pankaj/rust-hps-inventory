use sqlx::Row;
use tauri::State;

use crate::{db::AppState, models::*};

use super::logs::refresh_attendance_issues;
use super::{to_string, CommandResult};

#[tauri::command]
pub async fn list_admin_alerts(state: State<'_, AppState>) -> CommandResult<Vec<AdminAlert>> {
    refresh_attendance_issues(&state.db).await?;
    let rows = sqlx::query(
        r#"
        SELECT id, severity, kind, message, entity_table, entity_id, resolved, created_at
        FROM admin_notifications
        WHERE resolved = 0
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(alert_from_row).collect())
}

#[tauri::command]
pub async fn resolve_alert(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("UPDATE admin_notifications SET resolved = 1 WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

fn alert_from_row(row: sqlx::sqlite::SqliteRow) -> AdminAlert {
    AdminAlert {
        id: row.get("id"),
        severity: row.get("severity"),
        kind: row.get("kind"),
        message: row.get("message"),
        entity_table: row.get("entity_table"),
        entity_id: row.get("entity_id"),
        resolved: row.get::<i64, _>("resolved") != 0,
        created_at: row.get("created_at"),
    }
}

