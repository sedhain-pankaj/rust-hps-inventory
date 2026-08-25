use sqlx::Row;
use tauri::State;

use crate::{db::AppState, models::*};

use super::{to_string, CommandResult};

#[tauri::command]
pub async fn list_stock_items(state: State<'_, AppState>) -> CommandResult<Vec<StockItem>> {
    let rows = sqlx::query(
        r#"
        SELECT id, item_type, model, stock, reserved, location, dimensions, photo_path, notes
        FROM stock_items
        ORDER BY item_type, model COLLATE NOCASE
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;

    Ok(rows
        .into_iter()
        .map(|row| StockItem {
            id: row.get("id"),
            item_type: row.get("item_type"),
            model: row.get("model"),
            stock: row.get("stock"),
            reserved: row.get("reserved"),
            location: row.get("location"),
            dimensions: row.get("dimensions"),
            photo_path: row.get("photo_path"),
            notes: row.get("notes"),
        })
        .collect())
}

#[tauri::command]
pub async fn save_stock_item(
    state: State<'_, AppState>,
    input: StockItemInput,
) -> CommandResult<StockItem> {
    if input.model.trim().is_empty() {
        return Err("Model is required.".to_string());
    }
    if input.stock < 0 {
        return Err("Stock cannot be negative.".to_string());
    }
    if input.reserved < 0 {
        return Err("Reserved cannot be negative.".to_string());
    }
    let now = crate::db::now_string();
    let id = if let Some(id) = input.id {
        sqlx::query(
            r#"
            UPDATE stock_items
            SET item_type = ?, model = ?, stock = ?, reserved = ?, location = ?, dimensions = ?,
                photo_path = ?, notes = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(input.item_type.trim())
        .bind(input.model.trim())
        .bind(input.stock)
        .bind(input.reserved)
        .bind(input.location.trim())
        .bind(input.dimensions.trim())
        .bind(input.photo_path.trim())
        .bind(input.notes.trim())
        .bind(&now)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        id
    } else {
        let result = sqlx::query(
            r#"
            INSERT INTO stock_items
                (item_type, model, stock, reserved, location, dimensions, photo_path, notes, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(input.item_type.trim())
        .bind(input.model.trim())
        .bind(input.stock)
        .bind(input.reserved)
        .bind(input.location.trim())
        .bind(input.dimensions.trim())
        .bind(input.photo_path.trim())
        .bind(input.notes.trim())
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
        result.last_insert_rowid()
    };

    stock_item_by_id(&state.db, id).await
}

#[tauri::command]
pub async fn delete_stock_item(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    sqlx::query("DELETE FROM stock_items WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(to_string)?;
    Ok(())
}

async fn stock_item_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<StockItem> {
    let row = sqlx::query(
        "SELECT id, item_type, model, stock, reserved, location, dimensions, photo_path, notes FROM stock_items WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;

    Ok(StockItem {
        id: row.get("id"),
        item_type: row.get("item_type"),
        model: row.get("model"),
        stock: row.get("stock"),
        reserved: row.get("reserved"),
        location: row.get("location"),
        dimensions: row.get("dimensions"),
        photo_path: row.get("photo_path"),
        notes: row.get("notes"),
    })
}

