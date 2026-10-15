//! The three senders a delivery run uses, loaded once per send.
//!
//! [`Channels::load`] reads everything a run needs in a handful of queries —
//! device tokens, addresses, Slack links, the content — so the per-recipient
//! loop in [`super::delivery`] does no lookups of its own. Content is read from
//! the org's store only when email or Slack needs it: a push carries none.

use std::collections::HashMap;
use std::sync::Arc;

use entity::{
    device_tokens, notification_sends, notifications, organizations, slack_user_links, users,
};
use oxy_app_core::custom_app_environment::AppEnvironment;
use oxy_oltp::inbox::{self, InboxMessage};
use oxy_slack_client::SlackClient;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use super::deliver::{Push, PushSignal, push};
use super::delivery::Channel;
use super::platform::render;
use crate::emails::app_emailer::{AppEmailer, EmailSendInput};
use crate::integrations::slack::services::installations::InstallationsService;

/// How one channel went for one person. Reasons are fixed codes, stored as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelOutcome {
    Sent,
    Skipped(&'static str),
    Failed(&'static str),
}

struct SlackDm {
    client: SlackClient,
    bot_token: String,
    /// Oxy user → Slack user id, for the people linked in this org's Slack.
    links: HashMap<Uuid, String>,
}

pub struct Channels {
    kind: String,
    push: Arc<dyn Push>,
    tokens: HashMap<Uuid, Vec<device_tokens::Model>>,
    /// Present only when the send asked for email.
    email: Option<(AppEmailer, HashMap<Uuid, String>)>,
    /// Present only when the send asked for Slack and the org has it.
    slack: Option<SlackDm>,
    content: Option<InboxMessage>,
    url: Option<String>,
}

impl Channels {
    pub async fn load(
        db: &DatabaseConnection,
        send: &notification_sends::Model,
        signals: &[notifications::Model],
    ) -> Result<Self, String> {
        let users: Vec<Uuid> = signals.iter().map(|s| s.user_id).collect();
        let tokens = tokens(db, &users).await?;
        let content = content(db, send, signals).await?;
        let (app_name, url) = names_and_url(db, send, content.as_ref()).await?;
        let email = if send.email {
            Some((AppEmailer::from_env(app_name), addresses(db, &users).await?))
        } else {
            None
        };
        let slack = if send.slack {
            slack_dm(send.org_id, &users).await?
        } else {
            None
        };
        Ok(Self {
            kind: send.kind.clone(),
            push: push(),
            tokens,
            email,
            slack,
            content,
            url,
        })
    }

    pub async fn send(&self, channel: Channel, user: Uuid) -> ChannelOutcome {
        match channel {
            Channel::Push => self.push(user).await,
            Channel::Email => self.email(user).await,
            Channel::Slack => self.slack(user).await,
        }
    }

    async fn push(&self, user: Uuid) -> ChannelOutcome {
        if !self.push.configured() {
            return ChannelOutcome::Skipped("not_configured");
        }
        let Some(tokens) = self.tokens.get(&user).filter(|t| !t.is_empty()) else {
            return ChannelOutcome::Skipped("no_devices");
        };
        let signal = PushSignal {
            user_id: user,
            kind: self.kind.clone(),
        };
        let report = self.push.send(tokens, &signal).await;
        match (report.delivered, report.failed) {
            (0, 0) => ChannelOutcome::Skipped("no_devices"),
            (0, _) => ChannelOutcome::Failed("provider_error"),
            _ => ChannelOutcome::Sent,
        }
    }

    async fn email(&self, user: Uuid) -> ChannelOutcome {
        let Some((emailer, addresses)) = &self.email else {
            return ChannelOutcome::Skipped("not_requested");
        };
        let Some(to) = addresses.get(&user) else {
            // A frontline worker, or a deleted account: nobody to mail.
            return ChannelOutcome::Skipped("no_address");
        };
        let Some(content) = &self.content else {
            return ChannelOutcome::Failed("content_unavailable");
        };
        let subject = content.email_subject.as_deref().unwrap_or(&content.title);
        let input = EmailSendInput::to_one(
            to,
            subject,
            content.email_html.clone(),
            content
                .email_text
                .clone()
                .or_else(|| Some(content.title.clone())),
        );
        match emailer.send(input).await {
            Ok(_) => ChannelOutcome::Sent,
            Err(e) => {
                // The provider's text can carry the address, so it goes to the
                // log, never to the ledger.
                tracing::warn!(%user, error = %e, "notification email failed");
                ChannelOutcome::Failed("provider_error")
            }
        }
    }

    async fn slack(&self, user: Uuid) -> ChannelOutcome {
        let Some(dm) = &self.slack else {
            return ChannelOutcome::Skipped("not_configured");
        };
        let Some(slack_user) = dm.links.get(&user) else {
            return ChannelOutcome::Skipped("not_linked");
        };
        let Some(content) = &self.content else {
            return ChannelOutcome::Failed("content_unavailable");
        };
        let text = slack_text(content, self.url.as_deref());
        match dm
            .client
            .chat_post_message(&dm.bot_token, slack_user, &text, None)
            .await
        {
            Ok(_) => ChannelOutcome::Sent,
            Err(e) => {
                tracing::warn!(%user, error = %e, "notification Slack DM failed");
                ChannelOutcome::Failed("provider_error")
            }
        }
    }
}

/// What email and Slack say, when the send asked for either: an app's from
/// the org's store, a platform notice's rendered from Oxy's records.
async fn content(
    db: &DatabaseConnection,
    send: &notification_sends::Model,
    signals: &[notifications::Model],
) -> Result<Option<InboxMessage>, String> {
    if !(send.email || send.slack) {
        return Ok(None);
    }
    if let Some(signal) = signals.first().filter(|s| render::is_platform(s)) {
        return render::message(db, signal)
            .await
            .map(Some)
            .map_err(|e| format!("render the notice: {e}"));
    }
    Ok(inbox::read(db, send.org_id, &[send.id])
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next())
}

async fn tokens(
    db: &DatabaseConnection,
    users: &[Uuid],
) -> Result<HashMap<Uuid, Vec<device_tokens::Model>>, String> {
    let mut out: HashMap<Uuid, Vec<device_tokens::Model>> = HashMap::new();
    for t in device_tokens::Entity::find()
        .filter(device_tokens::Column::UserId.is_in(users.iter().copied()))
        .all(db)
        .await
        .map_err(|e| format!("load device tokens: {e}"))?
    {
        out.entry(t.user_id).or_default().push(t);
    }
    Ok(out)
}

/// The mailbox of every recipient who has one and an active account.
async fn addresses(
    db: &DatabaseConnection,
    users: &[Uuid],
) -> Result<HashMap<Uuid, String>, String> {
    Ok(users::Entity::find()
        .filter(users::Column::Id.is_in(users.iter().copied()))
        .filter(users::Column::Status.eq(users::UserStatus::Active))
        .all(db)
        .await
        .map_err(|e| format!("load addresses: {e}"))?
        .into_iter()
        .filter_map(|u| {
            let email = u.email?.trim().to_string();
            (!email.is_empty()).then_some((u.id, email))
        })
        .collect())
}

/// The sender's display name and the absolute link email and Slack carry.
async fn names_and_url(
    db: &DatabaseConnection,
    send: &notification_sends::Model,
    content: Option<&InboxMessage>,
) -> Result<(String, Option<String>), String> {
    let app = match send.app_id {
        Some(id) => entity::apps::Entity::find_by_id(id)
            .one(db)
            .await
            .map_err(|e| format!("load the app: {e}"))?,
        None => None,
    };
    let org_slug = organizations::Entity::find_by_id(send.org_id)
        .one(db)
        .await
        .map_err(|e| format!("load the org: {e}"))?
        .map(|o| o.slug);
    let environment =
        AppEnvironment::parse(&send.environment).unwrap_or(AppEnvironment::Production);
    let url = org_slug.as_deref().and_then(|org| {
        super::links::absolute_url(
            org,
            app.as_ref().map(|a| a.slug.as_str()),
            &environment,
            content.and_then(|c| c.link.as_deref()),
        )
    });
    let name = app.map(|a| a.name).unwrap_or_else(|| "Oxygen".to_string());
    Ok((name, url))
}

/// The org's Slack, and which recipients are linked in it. `None` when the org
/// has not connected Slack — every Slack delivery is then `not_configured`.
async fn slack_dm(org_id: Uuid, users: &[Uuid]) -> Result<Option<SlackDm>, String> {
    let Some(installation) = InstallationsService::find_active_by_org(org_id)
        .await
        .map_err(|e| format!("load the Slack installation: {e}"))?
    else {
        return Ok(None);
    };
    let bot_token = InstallationsService::decrypt_bot_token(&installation)
        .await
        .map_err(|e| format!("open the Slack bot token: {e}"))?;
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| format!("database: {e}"))?;
    let links = slack_user_links::Entity::find()
        .filter(slack_user_links::Column::InstallationId.eq(installation.id))
        .filter(slack_user_links::Column::OxyUserId.is_in(users.iter().copied()))
        .all(&db)
        .await
        .map_err(|e| format!("load Slack links: {e}"))?
        .into_iter()
        .map(|l| (l.oxy_user_id, l.slack_user_id))
        .collect();
    Ok(Some(SlackDm {
        client: SlackClient::new(),
        bot_token,
        links,
    }))
}

/// Slack's three reserved characters, escaped as Slack documents.
fn slack_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn slack_text(content: &InboxMessage, url: Option<&str>) -> String {
    let mut text = format!("*{}*", slack_escape(&content.title));
    if let Some(body) = &content.body {
        text.push('\n');
        text.push_str(&slack_escape(body));
    }
    if let Some(url) = url {
        // `<url|label>` has no escape for `|`.
        text.push_str(&format!("\n<{}|Open>", url.replace('|', "%7C")));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(title: &str, body: Option<&str>) -> InboxMessage {
        InboxMessage {
            id: Uuid::nil(),
            app_id: None,
            kind: "k".into(),
            title: title.into(),
            body: body.map(str::to_string),
            link: None,
            email_subject: None,
            email_html: None,
            email_text: None,
        }
    }

    #[test]
    fn slack_text_cannot_be_turned_into_a_mention_or_a_link() {
        let text = slack_text(&message("<!channel> & co", Some("<https://x|y>")), None);
        assert_eq!(text, "*&lt;!channel&gt; &amp; co*\n&lt;https://x|y&gt;");
    }

    #[test]
    fn slack_text_links_once_with_a_safe_url() {
        let text = slack_text(&message("Done", None), Some("https://a.example/x|y"));
        assert_eq!(text, "*Done*\n<https://a.example/x%7Cy|Open>");
    }
}
