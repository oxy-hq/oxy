//! /oxy <query> command handler

use crate::errors::OxyError;
use crate::slack::events::app_mention::handle_app_mention;

/// Handle `/oxy <query>` command
///
/// Treats the command text as a query and routes it through the same
/// logic as an app mention.
pub async fn handle_query_command(
    team_id: &str,
    channel_id: &str,
    user_id: &str,
    query_text: &str,
    response_url: &str,
) -> Result<String, OxyError> {
    // Spawn background task to handle the query
    // We need to return immediately to Slack (within 3 seconds)
    let team_id = team_id.to_string();
    let channel_id = channel_id.to_string();
    let user_id = user_id.to_string();
    let query_text = query_text.to_string();
    let response_url = response_url.to_string();

    tokio::spawn(async move {
        // Use current timestamp as event_ts since this is not a real event
        let event_ts = chrono::Utc::now().timestamp().to_string();

        if let Err(e) = handle_app_mention(
            &team_id,
            &channel_id,
            &user_id,
            &query_text,
            None, // No thread_ts for slash commands
            &event_ts,
        )
        .await
        {
            tracing::error!("Failed to handle query command: {}", e);
            post_error_to_slack(&response_url, &e.to_string()).await;
        }
    });

    Ok("🤔 Processing your query...".to_string())
}

/// Post an error message back to Slack via response_url
async fn post_error_to_slack(response_url: &str, error: &str) {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "response_type": "ephemeral",
        "text": format!("❌ Error: {}", error)
    });

    if let Err(e) = client.post(response_url).json(&payload).send().await {
        tracing::error!("Failed to post error to Slack response_url: {}", e);
    }
}
