mod commands;
mod db;
mod fingerprint;
mod models;

use tauri::{Manager, WindowEvent};

use commands::*;

pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(db::AppState::initialize(&handle))?;
            app.manage(state);

            if let Some(_window) = app.get_webview_window("main") {
                // let _ = window.set_fullscreen(true);
                // let _ = window.set_decorations(false);
                // let _ = window.set_always_on_top(true);
                // let _ = window.set_focus();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_status,
            get_asset_data_url,
            list_staff,
            save_employee,
            authenticate_password,
            authenticate_fingerprint,
            enroll_fingerprint,
            start_fingerprint_enroll,
            poll_fingerprint_enroll,
            read_fingerprint_progress,
            clear_fingerprint_progress,
            list_stock_items,
            save_stock_item,
            list_cornice_rates,
            save_cornice_rate,
            record_clock_event,
            list_clock_events,
            attendance_today,
            attendance_for_week,
            list_admin_alerts,
            resolve_alert,
            list_admin_tables,
            list_admin_table_rows,
            save_admin_table_row,
            delete_admin_table_row,
            add_cornice_log,
            list_cornice_logs,
            add_production_log,
            list_production_logs,
            add_overstock,
            list_overstock,
            add_delivery,
            list_deliveries,
            // New: mould inventory
            list_mould_inventory,
            save_mould_inventory,
            // New: cornice stock
            list_cornice_stock,
            save_cornice_stock,
            // New: clock event editing
            edit_clock_event,
            list_clock_event_edits,
            // New: fuzzy search
            search_cornice_rates,
            // New: payroll
            get_payroll_week,
            get_all_payroll_week,
            resolve_unknown_rate,
            override_payroll_proration,
            // New: dispatch orders
            list_dispatch_orders,
            create_dispatch_order,
            update_dispatch_order,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Hopkins kiosk");

    app.run(|app_handle, event| match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            api.prevent_exit();
        }
        tauri::RunEvent::WindowEvent {
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            api.prevent_close();
        }
        tauri::RunEvent::WindowEvent {
            event: WindowEvent::Focused(false),
            ..
        } => {
            if let Some(_window) = app_handle.get_webview_window("main") {
                // let _ = window.show();
                // let _ = window.unminimize();
                // let _ = window.set_fullscreen(true);
                // let _ = window.set_always_on_top(true);
                // let _ = window.set_focus();
            }
        }
        _ => {}
    });
}
