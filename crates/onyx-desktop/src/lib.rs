use command::{ask, get_openai_api_key, list_chat_messages, set_openai_api_key};

mod command;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_chat_messages,
            ask,
            get_openai_api_key,
            set_openai_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
