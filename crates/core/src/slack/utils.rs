//! Slack utility functions and shared patterns

use once_cell::sync::Lazy;

// =============================================================================
// Bot Mention Patterns
// =============================================================================

/// Regex pattern for Slack bot mentions (e.g., <@U12345678>)
static BOT_MENTION_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"<@[A-Z0-9]+>").unwrap());

/// Check if message text contains a bot mention (e.g., <@U12345678>)
pub fn contains_bot_mention(text: &str) -> bool {
    BOT_MENTION_RE.is_match(text)
}

/// Strip bot mention from message text
pub fn strip_bot_mention(text: &str) -> String {
    BOT_MENTION_RE.replace_all(text, "").trim().to_string()
}
