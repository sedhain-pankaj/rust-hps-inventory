use sqlx::Row;
use tauri::State;

use crate::{db::AppState, models::*};

use super::{to_string, CommandResult};

// ==================== Dispatch Orders ====================

#[tauri::command]
pub async fn list_dispatch_orders(
    state: State<'_, AppState>,
    status: Option<String>,
) -> CommandResult<Vec<DispatchOrder>> {
    if let Some(s) = status {
        let rows = sqlx::query(
            r#"
            SELECT d.*, e.name AS created_by_name, de.name AS delivered_by_name
            FROM dispatch_orders d
            JOIN employees e ON e.id = d.created_by
            LEFT JOIN employees de ON de.id = d.delivered_by
            WHERE d.status = ?
            ORDER BY d.created_at DESC, d.id DESC
            "#,
        )
        .bind(s)
        .fetch_all(&state.db)
        .await
        .map_err(to_string)?;
        return Ok(rows.into_iter().map(dispatch_order_from_row).collect());
    }

    let rows = sqlx::query(
        r#"
        SELECT d.*, e.name AS created_by_name, de.name AS delivered_by_name
        FROM dispatch_orders d
        JOIN employees e ON e.id = d.created_by
        LEFT JOIN employees de ON de.id = d.delivered_by
        ORDER BY d.created_at DESC, d.id DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_string)?;
    Ok(rows.into_iter().map(dispatch_order_from_row).collect())
}

#[tauri::command]
pub async fn create_dispatch_order(
    state: State<'_, AppState>,
    input: DispatchOrderInput,
    created_by: String,
) -> CommandResult<DispatchOrder> {
    if input.cornice_model.trim().is_empty() {
        return Err("Cornice model is required.".to_string());
    }
    if input.quantity <= 0 {
        return Err("Quantity must be positive.".to_string());
    }
    if input.delivery_location.trim().is_empty() {
        return Err("Delivery location is required.".to_string());
    }

    let now = crate::db::now_string();
    let result = sqlx::query(
        r#"
        INSERT INTO dispatch_orders (cornice_model, quantity, delivery_location, status, created_by, remarks, created_at)
        VALUES (?, ?, ?, 'pending', ?, ?, ?)
        "#,
    )
    .bind(input.cornice_model.trim())
    .bind(input.quantity)
    .bind(input.delivery_location.trim())
    .bind(created_by.trim())
    .bind(input.remarks.trim())
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(to_string)?;

    dispatch_order_by_id(&state.db, result.last_insert_rowid()).await
}

#[tauri::command]
pub async fn update_dispatch_order(
    state: State<'_, AppState>,
    input: DispatchOrderInput,
    updated_by: String,
) -> CommandResult<DispatchOrder> {
    let id = input.id.ok_or_else(|| "Order ID is required.".to_string())?;

    let mut assignments = Vec::new();
    let mut binds: Vec<&str> = Vec::new();

    if let Some(status) = &input.status {
        if !["pending", "in_progress", "delivered"].contains(&status.trim()) {
            return Err("Status must be 'pending', 'in_progress', or 'delivered'.".to_string());
        }
        assignments.push("status = ?");
        binds.push(status.trim());
    }
    if !input.remarks.trim().is_empty() {
        assignments.push("remarks = ?");
        binds.push(input.remarks.trim());
    }

    // If marking as delivered, set delivered_by and delivered_at
    let now_str = crate::db::now_string();
    if input.status.as_deref() == Some("delivered") {
        assignments.push("delivered_by = ?");
        binds.push(updated_by.trim());
        assignments.push("delivered_at = ?");
        binds.push(&now_str);
    }

    if assignments.is_empty() {
        return dispatch_order_by_id(&state.db, id).await;
    }

    let placeholders = assignments.join(", ");
    let sql = format!("UPDATE dispatch_orders SET {placeholders} WHERE id = ?");
    let mut query = sqlx::query(&sql);
    for b in binds {
        query = query.bind(b);
    }
    query.bind(id).execute(&state.db).await.map_err(to_string)?;

    dispatch_order_by_id(&state.db, id).await
}

fn dispatch_order_from_row(row: sqlx::sqlite::SqliteRow) -> DispatchOrder {
    DispatchOrder {
        id: row.get("id"),
        cornice_model: row.get("cornice_model"),
        quantity: row.get("quantity"),
        delivery_location: row.get("delivery_location"),
        status: row.get("status"),
        created_by_id: row.get("created_by"),
        created_by_name: row.get("created_by_name"),
        delivered_by_name: row.get("delivered_by_name"),
        delivered_at: row.get("delivered_at"),
        remarks: row.get("remarks"),
        created_at: row.get("created_at"),
    }
}

async fn dispatch_order_by_id(db: &sqlx::SqlitePool, id: i64) -> CommandResult<DispatchOrder> {
    let row = sqlx::query(
        r#"
        SELECT d.*, e.name AS created_by_name, de.name AS delivered_by_name
        FROM dispatch_orders d
        JOIN employees e ON e.id = d.created_by
        LEFT JOIN employees de ON de.id = d.delivered_by
        WHERE d.id = ?
        "#,
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(to_string)?;
    Ok(dispatch_order_from_row(row))
}

