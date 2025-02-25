use command::{ask, ask_preview, get_openai_api_key, list_chat_messages, set_openai_api_key};
use migration::Migrator;
use migration::MigratorTrait;
use onyx::db::client;

mod command;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Webview,
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Folder {
                        path: std::path::PathBuf::from(client::get_state_dir()),
                        file_name: Some("onyx-desktop".to_string()),
                    },
                ))
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .setup(|app| {
            tauri::async_runtime::block_on(async move {
                // create db directory if not exists
                let db = client::establish_connection().await;
                // migrate db
                let _ = Migrator::up(&db, None).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_chat_messages,
            ask,
            ask_preview,
            get_openai_api_key,
            set_openai_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
