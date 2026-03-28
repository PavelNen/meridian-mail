use std::path::PathBuf;

use tauri::Manager;

mod commands;
mod db;
mod error;
mod imap;
mod keychain;
mod models;
mod smtp;

use commands::AppState;

// ─── App data directory ───────────────────────────────────────────────────────

fn db_path(app: &tauri::App) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("failed to resolve app data dir")
        .join("meridian-mail.db")
}

// ─── Entry point ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Ensure the app data directory exists.
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;

            // Open (or create) the SQLite database.
            let path = db_path(app);
            log::info!("Opening database at: {}", path.display());
            let db = db::Database::open(&path).expect("failed to open database");



            app.manage(AppState::new(db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Account management
            commands::add_account,
            commands::list_accounts,
            commands::remove_account,
            commands::list_macos_mail_accounts,
            // IMAP
            commands::test_imap_connection,
            commands::get_inbox_info,
            commands::sync_inbox,
            commands::start_idle_sync,
            // Conversations
            commands::list_conversations,
            commands::mark_conversation_read,
            commands::delete_conversation,
            // Messages
            commands::get_messages,
            commands::search_messages,
            commands::delete_message,
            // Contacts
            commands::list_contacts,
            commands::search_contacts,
            // Send
            commands::send_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
