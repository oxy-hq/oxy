//! Mutes for platform notices: one switch per (kind, channel).
//!
//! Kept beside the app mutes rather than in them because
//! `notification_mutes.app_id` is `NOT NULL` with a foreign key to `apps`.
//! The catalogue works the same way: the kinds a person has received, plus any
//! they muted. A critical kind cannot be muted at all; the app side stores
//! such a mute and ignores it, but a platform kind is critical in code, not in
//! a manifest that might change, so a mute of one could never apply. Nor
//! can a test notice's (`kinds::is_mutable`), which is left out of the list:
//! a switch that does nothing is noise.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use chrono::Utc;
use entity::{notification_platform_mutes as m, notifications};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect, Set};
use uuid::Uuid;

use super::kinds::{TEST, channels, is_critical, is_mutable, is_platform_kind};
use crate::server::api::notifications::delivery::Channel;
use crate::server::api::notifications::preferences::KindPreference;

/// The platform kinds `user_id` has received or muted, with their mutes.
pub async fn kinds(db: &DatabaseConnection, user_id: Uuid) -> Result<Vec<KindPreference>, DbErr> {
    let received: Vec<String> = notifications::Entity::find()
        .select_only()
        .column(notifications::Column::Kind)
        .filter(notifications::Column::UserId.eq(user_id))
        .filter(notifications::Column::AppId.is_null())
        .distinct()
        .into_tuple::<String>()
        .all(db)
        .await?;
    let mut by_kind: BTreeMap<String, Vec<&'static str>> = received
        .into_iter()
        .filter(|k| is_platform_kind(k))
        .map(|k| (k, Vec::new()))
        .collect();
    for mute in m::Entity::find()
        .filter(m::Column::UserId.eq(user_id))
        .all(db)
        .await?
    {
        if let Some(c) = Channel::parse(&mute.channel) {
            by_kind.entry(mute.kind).or_default().push(c.as_str());
        }
    }
    Ok(by_kind
        .into_iter()
        // No switch for a test notice: nothing would honour it.
        .filter(|(kind, _)| kind != TEST)
        .map(|(kind, muted)| KindPreference {
            critical: is_critical(&kind),
            channels: channels(&kind).to_vec(),
            kind,
            muted,
        })
        .collect())
}

/// Mute or unmute `channel` for platform `kind`. `400` for a kind the
/// platform does not send, a channel it is never delivered on, or for muting
/// one no mute applies to (critical, or a test notice).
pub async fn set(
    db: &DatabaseConnection,
    user_id: Uuid,
    kind: &str,
    channel: Channel,
    muted: bool,
) -> Result<StatusCode, StatusCode> {
    if !is_platform_kind(kind)
        || (muted && !is_mutable(kind))
        || !channels(kind).contains(&channel.as_str())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    write(db, user_id, kind, channel, muted)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn write(
    db: &DatabaseConnection,
    user_id: Uuid,
    kind: &str,
    channel: Channel,
    muted: bool,
) -> Result<(), DbErr> {
    if muted {
        m::Entity::insert(m::ActiveModel {
            user_id: Set(user_id),
            kind: Set(kind.to_string()),
            channel: Set(channel.as_str().to_string()),
            created_at: Set(Utc::now().fixed_offset()),
        })
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([
                m::Column::UserId,
                m::Column::Kind,
                m::Column::Channel,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    } else {
        m::Entity::delete_many()
            .filter(m::Column::UserId.eq(user_id))
            .filter(m::Column::Kind.eq(kind))
            .filter(m::Column::Channel.eq(channel.as_str()))
            .exec(db)
            .await?;
    }
    Ok(())
}

/// Who muted which channel for a platform send of `kind`. Empty for a kind
/// no mute applies to.
pub async fn muted(
    db: &DatabaseConnection,
    kind: &str,
    users: &[Uuid],
) -> Result<std::collections::HashSet<(Uuid, Channel)>, DbErr> {
    if !is_mutable(kind) {
        return Ok(Default::default());
    }
    Ok(m::Entity::find()
        .filter(m::Column::Kind.eq(kind))
        .filter(m::Column::UserId.is_in(users.iter().copied()))
        .all(db)
        .await?
        .into_iter()
        .filter_map(|row| Channel::parse(&row.channel).map(|c| (row.user_id, c)))
        .collect())
}
