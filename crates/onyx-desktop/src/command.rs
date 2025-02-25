use futures::StreamExt;
use onyx::service::{
    agent::{AskRequest, Message},
    message::MessageItem,
};
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri_plugin_store::StoreExt;

const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
const STORE_FILE_NAME: &str = "store.json";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum MessageEvent {
    #[serde(rename_all = "camelCase")]
    OnMessage {
        message: Message,
    },
    OnComplete,
}

#[tauri::command]
pub async fn get_openai_api_key(app_handle: tauri::AppHandle) -> Result<String, String> {
    let store = app_handle
        .store(STORE_FILE_NAME)
        .map_err(|_| "Failed to open store")?;

    let openai_api_key_value = store
        .get(OPENAI_API_KEY_ENV_VAR)
        .unwrap_or_else(|| Value::String("".to_string()));

    let mut openai_api_key = openai_api_key_value
        .as_str()
        .unwrap_or_else(|| "")
        .to_string();

    if openai_api_key.is_empty() {
        match std::env::var(OPENAI_API_KEY_ENV_VAR) {
            Ok(api_key) => {
                println!("api_key: {:?}", api_key);
                if !api_key.is_empty() {
                    openai_api_key = api_key.clone();
                    store.set(OPENAI_API_KEY_ENV_VAR, Value::String(api_key.clone()));
                }
            }
            Err(_) => store.set(OPENAI_API_KEY_ENV_VAR, Value::String("".to_string())),
        }
    }

    std::env::set_var(OPENAI_API_KEY_ENV_VAR, openai_api_key.clone());

    Ok(openai_api_key.to_string())
}

#[tauri::command]
pub async fn set_openai_api_key(
    app_handle: tauri::AppHandle,
    key: String,
) -> Result<String, String> {
    let store = app_handle
        .store(STORE_FILE_NAME)
        .map_err(|_| "Failed to open store")?;

    store.set(OPENAI_API_KEY_ENV_VAR, Value::String(key.clone()));

    std::env::set_var(OPENAI_API_KEY_ENV_VAR, key.clone());

    Ok(key.to_string())
}

#[tauri::command]
pub async fn list_chat_messages(agent_path: String) -> Result<Vec<MessageItem>, String> {
    let res = onyx::service::message::get_messages(agent_path).await;
    match res {
        Ok(res) => Ok(res),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn ask(request: AskRequest, on_event: Channel<MessageEvent>) -> Result<(), ()> {
    let res = onyx::service::agent::ask(request).await;
    let mut stream = Box::pin(res);
    while let Some(value) = stream.next().await {
        on_event
            .send(MessageEvent::OnMessage { message: value })
            .unwrap();
    }
    on_event.send(MessageEvent::OnComplete).unwrap();
    Ok(())
}

#[tauri::command]
pub async fn ask_preview(request: AskRequest, on_event: Channel<MessageEvent>) -> Result<(), ()> {
    let res = onyx::service::agent::ask_preview(request).await;
    let mut stream = Box::pin(res);
    while let Some(value) = stream.next().await {
        on_event
            .send(MessageEvent::OnMessage { message: value })
            .unwrap();
    }
    on_event.send(MessageEvent::OnComplete).unwrap();
    Ok(())
}
