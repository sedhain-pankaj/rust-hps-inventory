use sqlx::Row;
use tauri::State;

use crate::{db::AppState, models::*};

use super::{to_string, CommandResult};

// ==================== Mould Inventory ====================

#[tauri::command]
pub async fn list_mould_inventory(state: State<'_, AppState>) -> CommandResult<Vec<MouldInventory>> {
    let rows = sqlx::query(
        r#"
        SELECT id, mould_name, storage_location, notes, column_id, updated_at
        FROM mould_inventory
        ORDER BY mould_name COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows.into_iter().map(mould_from_row).collect())
}

#[tauri::command]
pub async fn save_mould_inventory(
    state: State<'_, AppState>,
    input: MouldInventoryInput,
) -> CommandResult<MouldInventory> {
    if input.mould_name.trim().is_empty() {
        return Err("Mould name is required.".to_string());
    }
    let storage_location = match input.column_id {
        Some(column_id) => {
            let row: Option<(String,)> = sqlx::query_as(
                r#"
                SELECT l.name
                FROM mould_location_columns c
                JOIN mould_locations l ON l.id = c.location_id
                WHERE c.id = ?
                "#,
            )
            .bind(column_id)
            .fetch_optional(&state.db)
            .await
            .map_err(to_string)?;
            row.map(|(name,)| name)
                .ok_or_else(|| "Unknown mould column.".to_string())?
        }
        None => String::new(),
    };
    let now = crate::db::now_string();
    let id = if let Some(id) = input.id {
        sqlx::query(
            r#"
            UPDATE mould_inventory
            SET mould_name = ?, storage_location = ?, column_id = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.mould_name.trim())
        .bind(&storage_location)
        .bind(input.column_id)
        .bind(&now)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO mould_inventory (mould_name, storage_location, column_id, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(input.mould_name.trim())
        .bind(&storage_location)
        .bind(input.column_id)
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    mould_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn delete_mould_inventory(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("DELETE FROM mould_inventory WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

// ==================== Mould Locations ====================

#[tauri::command]
pub async fn list_mould_locations(state: State<'_, AppState>) -> CommandResult<Vec<MouldLocation>> {
    let rows = sqlx::query("SELECT id, name, sort_order FROM mould_locations ORDER BY sort_order, id")
        .fetch_all(&state.db)
        .await
        .map_err(to_string)?;
    Ok(rows.into_iter().map(mould_location_from_row).collect())
}

#[tauri::command]
pub async fn save_mould_location(
    state: State<'_, AppState>,
    input: MouldLocationInput,
) -> CommandResult<MouldLocation> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("Location name is required.".to_string());
    }
    // Reject duplicate names (case-insensitive): the UI groups moulds by name and
    // the delete guard matches by name, so two locations sharing a name conflate.
    // id != 0 is a no-op for new locations (AUTOINCREMENT starts at 1) and excludes
    // the row itself on rename.
    let dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mould_locations WHERE name = ? COLLATE NOCASE AND id != ?)",
    )
    .bind(&name)
    .bind(input.id.unwrap_or(0))
    .fetch_one(&state.db)
    .await
    .map_err(to_string)?;
    if dup {
        return Err("A location with that name already exists.".to_string());
    }
    if let Some(id) = input.id {
        // Fetch the old name so the denormalized storage_location on unassigned
        // moulds (column_id IS NULL) stays in sync with the rename — otherwise
        // they display as "Unassigned" and the delete guard undercounts them.
        let old_name: Option<String> = sqlx::query_scalar(
            "SELECT name FROM mould_locations WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_string)?;
        sqlx::query("UPDATE mould_locations SET name = ? WHERE id = ?")
            .bind(&name)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        if let Some(old) = old_name {
            if old != name {
                sqlx::query(
                    "UPDATE mould_inventory SET storage_location = ? \
                     WHERE column_id IS NULL AND storage_location = ?",
                )
                .bind(&name)
                .bind(&old)
                .execute(&state.db)
                .await
                .map_err(to_string)?;
            }
        }
        let row = sqlx::query("SELECT id, name, sort_order FROM mould_locations WHERE id = ?")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(to_string)?;
        Ok(mould_location_from_row(row))
    } else {
        let next: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM mould_locations")
            .fetch_one(&state.db)
            .await
            .map_err(to_string)?;
        let result = sqlx::query("INSERT INTO mould_locations (name, sort_order) VALUES (?, ?)")
            .bind(&name)
            .bind(next)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        let location_id = result.last_insert_rowid();
        for index in 1..=5 {
            sqlx::query(
                "INSERT INTO mould_location_columns (location_id, name, sort_order) VALUES (?, ?, ?)",
            )
            .bind(location_id)
            .bind(format!("R{index}"))
            .bind(index - 1)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        }
        let row = sqlx::query("SELECT id, name, sort_order FROM mould_locations WHERE id = ?")
            .bind(location_id)
            .fetch_one(&state.db)
            .await
            .map_err(to_string)?;
        Ok(mould_location_from_row(row))
    }
}

#[tauri::command]
pub async fn delete_mould_location(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    let location: Option<(String,)> =
        sqlx::query_as("SELECT name FROM mould_locations WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(to_string)?;
    let (name,) = location.ok_or_else(|| "Location not found.".to_string())?;
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM mould_inventory
        WHERE column_id IN (SELECT id FROM mould_location_columns WHERE location_id = ?)
           OR (column_id IS NULL AND storage_location = ?)
        "#,
    )
    .bind(id)
    .bind(&name)
    .fetch_one(&state.db)
    .await
    .map_err(to_string)?;
    if count > 0 {
        return Err(format!("Cannot delete: {count} mould(s) are stored in this location."));
    }
    sqlx::query("DELETE FROM mould_location_columns WHERE location_id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    sqlx::query("DELETE FROM mould_locations WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

fn mould_location_from_row(row: sqlx::sqlite::SqliteRow) -> MouldLocation {
    MouldLocation {
        id: row.get("id"),
        name: row.get("name"),
        sort_order: row.get("sort_order"),
    }
}

// ==================== Mould Location Columns ====================

#[tauri::command]
pub async fn list_mould_location_columns(
    state: State<'_, AppState>,
) -> CommandResult<Vec<MouldLocationColumn>> {
    let rows = sqlx::query(
        "SELECT id, location_id, name, sort_order FROM mould_location_columns ORDER BY sort_order, id",
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;
    Ok(rows.into_iter().map(mould_location_column_from_row).collect())
}

#[tauri::command]
pub async fn save_mould_location_column(
    state: State<'_, AppState>,
    input: MouldLocationColumnInput,
) -> CommandResult<MouldLocationColumn> {
    let location_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mould_locations WHERE id = ?")
        .bind(input.location_id)
        .fetch_one(&state.db)
        .await
        .map_err(to_string)?;
    if location_count == 0 {
        return Err("Location not found.".to_string());
    }
    let id = if let Some(id) = input.id {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err("Column name is required.".to_string());
        }
        sqlx::query("UPDATE mould_location_columns SET name = ? WHERE id = ?")
            .bind(&name)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(to_string)?;
        id
    } else {
        // Empty name on create = auto-name: R{highest existing R index + 1}.
        let name = input.name.trim().to_string();
        let final_name = if name.is_empty() {
            let names: Vec<(String,)> = sqlx::query_as(
                "SELECT name FROM mould_location_columns WHERE location_id = ?",
            )
            .bind(input.location_id)
            .fetch_all(&state.db)
            .await
            .map_err(to_string)?;
            let max_index = names
                .iter()
                .filter_map(|(existing,)| existing.strip_prefix('R'))
                .filter_map(|digits| digits.parse::<i64>().ok())
                .max()
                .unwrap_or(0);
            format!("R{}", max_index + 1)
        } else {
            name
        };
        let next: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM mould_location_columns WHERE location_id = ?",
        )
        .bind(input.location_id)
        .fetch_one(&state.db)
        .await
        .map_err(to_string)?;
        let result = sqlx::query(
            "INSERT INTO mould_location_columns (location_id, name, sort_order) VALUES (?, ?, ?)",
        )
        .bind(input.location_id)
        .bind(&final_name)
        .bind(next)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    let row = sqlx::query(
        "SELECT id, location_id, name, sort_order FROM mould_location_columns WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(to_string)?;
    Ok(mould_location_column_from_row(row))
}

/// Guarded delete of a mould location column. Extracted from the Tauri command
/// so the rules are unit-testable without a `tauri::State`.
///
/// Rules:
/// - Only the last column (highest `sort_order`) of its location can be deleted,
///   so columns are removed from the end (R5 before R4, R4 before R3, …).
/// - A location must keep at least one column.
/// - Moulds in the deleted column are unassigned (`column_id = NULL`) and show
///   up under "Unassigned" until re-placed.
pub async fn delete_mould_location_column_checked(db: &sqlx::SqlitePool, id: i64) -> CommandResult<()> {
    let column: Option<(i64, i64)> = sqlx::query_as(
        "SELECT location_id, sort_order FROM mould_location_columns WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(to_string)?;
    let (location_id, sort_order) = column.ok_or_else(|| "Column not found.".to_string())?;
    let after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mould_location_columns
         WHERE location_id = ? AND (sort_order > ? OR (sort_order = ? AND id > ?))",
    )
    .bind(location_id)
    .bind(sort_order)
    .bind(sort_order)
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    if after > 0 {
        return Err(
            "Only the last column can be deleted. Delete columns from the end (e.g. R5 before R4)."
                .to_string(),
        );
    }
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mould_location_columns WHERE location_id = ?",
    )
    .bind(location_id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    if total <= 1 {
        return Err("A location must keep at least one column.".to_string());
    }
    sqlx::query("UPDATE mould_inventory SET column_id = NULL WHERE column_id = ?")
        .bind(id)
        .execute(db)
        .await
        .map_err(to_string)?;
    sqlx::query("DELETE FROM mould_location_columns WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .map_err(to_string)?;
    Ok(())
}

#[tauri::command]
pub async fn delete_mould_location_column(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    delete_mould_location_column_checked(&state.db, id).await
}

fn mould_location_column_from_row(row: sqlx::sqlite::SqliteRow) -> MouldLocationColumn {
    MouldLocationColumn {
        id: row.get("id"),
        location_id: row.get("location_id"),
        name: row.get("name"),
        sort_order: row.get("sort_order"),
    }
}

fn mould_from_row(row: sqlx::sqlite::SqliteRow) -> MouldInventory {
    MouldInventory {
        id: row.get("id"),
        mould_name: row.get("mould_name"),
        storage_location: row.get("storage_location"),
        notes: row.get("notes"),
        column_id: row.get("column_id"),
        updated_at: row.get("updated_at"),
    }
}

async fn mould_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<MouldInventory> {
    let row = sqlx::query(
        "SELECT id, mould_name, storage_location, notes, column_id, updated_at FROM mould_inventory WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(mould_from_row(row))
}

