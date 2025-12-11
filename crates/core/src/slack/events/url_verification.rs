//! URL verification handler for Slack Events API

use crate::slack::types::UrlVerification;
use serde_json::json;

/// Handle Slack URL verification challenge
///
/// When first configuring the Events API, Slack sends a verification request
/// with a challenge token that we must echo back.
pub fn handle_url_verification(verification: &UrlVerification) -> serde_json::Value {
    tracing::info!("Handling Slack URL verification challenge");
    json!({ "challenge": verification.challenge })
}
