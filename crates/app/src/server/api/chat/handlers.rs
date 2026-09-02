use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{DateTime, FixedOffset};
use entity::{chat_channel_members, chat_channels, chat_messages, users};
use futures::stream::Stream;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use super::delivery;
use super::dto::*;

/// Resolve a channel the caller is actually a member of.
///
/// **Every read and write goes through here.** Membership is not a branch a
/// handler can forget — it is the lookup itself, so a handler that skips it
/// has no channel to act on rather than acting on somebody else's.
///
/// Returns `None` for "no such channel" AND for "not a member", deliberately
/// collapsed: distinguishing them tells a caller that a channel they cannot see
/// exists, which is the whole thing channel membership is protecting.
pub async fn member_channel(
    db: &DatabaseConnection,
    channel_id: Uuid,
    user_id: Uuid,
) -> Option<chat_channels::Model> {
    let membership = chat_channel_members::Entity::find_by_id((channel_id, user_id))
        .one(db)
        .await
        .ok()??;
    let channel = chat_channels::Entity::find_by_id(membership.channel_id)
        .one(db)
        .await
        .ok()??;
    // A `chat_channel_members` row is NOT sufficient on its own. Its foreign
    // keys point at `chat_channels` and `users`, so nothing deletes it when
    // somebody leaves the org — a removed member would otherwise keep read and
    // write on that org's channels indefinitely.
    //
    // Checked here rather than by deleting the membership rows on removal,
    // because this fails closed: a removal path that forgets to clean up costs
    // a stale row, not an open door.
    if !in_org(db, user_id, channel.org_id).await {
        return None;
    }
    Some(channel)
}

/// Is this user still in the channel's org?
///
/// Standing is org membership OR active frontline enrolment. Both, because a
/// frontline worker holds no `org_members` row by design — they are enrolled by
/// PIN on a shared tablet — and they are exactly who these channels exist for.
/// Checking membership alone would lock out the primary audience while looking
/// perfectly correct in every test written against office users.
///
/// `status = 'active'` on the frontline side, so suspending a worker removes
/// their chat access with the same row that removes everything else.
pub async fn in_org(db: &DatabaseConnection, user_id: Uuid, org_id: Uuid) -> bool {
    let member = entity::org_members::Entity::find()
        .filter(entity::org_members::Column::OrgId.eq(org_id))
        .filter(entity::org_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .ok()
        .flatten()
        .is_some();
    if member {
        return true;
    }
    entity::org_frontline_members::Entity::find()
        .filter(entity::org_frontline_members::Column::OrgId.eq(org_id))
        .filter(entity::org_frontline_members::Column::UserId.eq(user_id))
        .filter(entity::org_frontline_members::Column::Status.eq("active"))
        .one(db)
        .await
        .ok()
        .flatten()
        .is_some()
}

/// The org ids a user has standing in, for the list path.
///
/// The set form of [`in_org`], and it has to stay the set form of it: two
/// definitions of "standing" is how `list_channels` ended up without the gate
/// `member_channel` already had.
async fn orgs_with_standing(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<std::collections::HashSet<Uuid>, sea_orm::DbErr> {
    let mut orgs: std::collections::HashSet<Uuid> = entity::org_members::Entity::find()
        .filter(entity::org_members::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.org_id)
        .collect();
    orgs.extend(
        entity::org_frontline_members::Entity::find()
            .filter(entity::org_frontline_members::Column::UserId.eq(user_id))
            .filter(entity::org_frontline_members::Column::Status.eq("active"))
            .all(db)
            .await?
            .into_iter()
            .map(|m| m.org_id),
    );
    Ok(orgs)
}

fn iso(t: DateTime<FixedOffset>) -> String {
    t.to_rfc3339()
}

/// Member counts for a set of channels, in one `GROUP BY`.
async fn count_by_channel(
    db: &DatabaseConnection,
    channel_ids: &[Uuid],
) -> Result<HashMap<Uuid, u64>, StatusCode> {
    #[derive(FromQueryResult)]
    struct Row {
        channel_id: Uuid,
        n: i64,
    }
    Ok(chat_channel_members::Entity::find()
        .select_only()
        .column(chat_channel_members::Column::ChannelId)
        .column_as(chat_channel_members::Column::UserId.count(), "n")
        .filter(chat_channel_members::Column::ChannelId.is_in(channel_ids.to_vec()))
        .group_by(chat_channel_members::Column::ChannelId)
        .into_model::<Row>()
        .all(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|r| (r.channel_id, r.n.max(0) as u64))
        .collect())
}

/// The newest message timestamp per channel, in one `GROUP BY`.
///
/// Tombstones count: a deleted message still occupies its slot in the thread, so
/// it is still the last thing that happened in the channel.
async fn last_message_by_channel(
    db: &DatabaseConnection,
    channel_ids: &[Uuid],
) -> Result<HashMap<Uuid, DateTime<FixedOffset>>, StatusCode> {
    #[derive(FromQueryResult)]
    struct Row {
        channel_id: Uuid,
        last_at: Option<DateTime<FixedOffset>>,
    }
    Ok(chat_messages::Entity::find()
        .select_only()
        .column(chat_messages::Column::ChannelId)
        .column_as(chat_messages::Column::CreatedAt.max(), "last_at")
        .filter(chat_messages::Column::ChannelId.is_in(channel_ids.to_vec()))
        .group_by(chat_messages::Column::ChannelId)
        .into_model::<Row>()
        .all(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .filter_map(|r| r.last_at.map(|t| (r.channel_id, t)))
        .collect())
}

/// The channels this user may see: their membership rows, narrowed to orgs they
/// still have standing in.
///
/// Separated from `list_channels` because it is the authorization decision, and
/// an authorization decision behind an extractor is one no test can reach. The
/// gate it applies is the same one `member_channel` applies to a single channel:
/// a `chat_channel_members` row outlives org membership — its foreign keys reach
/// `chat_channels` and `users` only — so a removed member would otherwise keep
/// seeing that org's channel names, topics, member counts and a LIVE unread
/// count. The messages are gated; a moving unread badge is still a running
/// activity signal for a tenant the caller was removed from.
///
/// Two queries regardless of channel count, and it narrows the id list before
/// the aggregates run, so the gate costs nothing downstream.
///
/// Uses `orgs_with_standing`, the set form of `in_org`, so the list gate and the
/// per-channel gate cannot disagree about who counts.
pub async fn visible_channels(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<(chat_channel_members::Model, chat_channels::Model)>, sea_orm::DbErr> {
    let memberships = chat_channel_members::Entity::find()
        .filter(chat_channel_members::Column::UserId.eq(user_id))
        .all(db)
        .await?;
    if memberships.is_empty() {
        return Ok(Vec::new());
    }

    let orgs = orgs_with_standing(db, user_id).await?;

    let ids: Vec<Uuid> = memberships.iter().map(|m| m.channel_id).collect();
    let channels: HashMap<Uuid, chat_channels::Model> = chat_channels::Entity::find()
        .filter(chat_channels::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .filter(|c| orgs.contains(&c.org_id))
        .map(|c| (c.id, c))
        .collect();

    Ok(memberships
        .into_iter()
        .filter_map(|m| channels.get(&m.channel_id).cloned().map(|c| (m, c)))
        .collect())
}

/// `GET /api/chat/channels` — every channel this user belongs to.
#[instrument(skip_all)]
pub async fn list_channels(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<ChannelSummary>>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let visible = visible_channels(&db, user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if visible.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let channel_ids: Vec<Uuid> = visible.iter().map(|(_, c)| c.id).collect();
    let memberships: Vec<chat_channel_members::Model> =
        visible.iter().map(|(m, _)| m.clone()).collect();
    let channels: HashMap<Uuid, chat_channels::Model> =
        visible.into_iter().map(|(_, c)| (c.id, c)).collect();

    let member_counts = count_by_channel(&db, &channel_ids).await?;
    let last_message_at = last_message_by_channel(&db, &channel_ids).await?;

    let mut out = Vec::with_capacity(memberships.len());
    for m in memberships {
        let Some(ch) = channels.get(&m.channel_id) else {
            continue;
        };

        // Unread stays a COUNT against `last_read_at` rather than a stored
        // counter: a counter has to be maintained by every writer and drifts the
        // first time one fails, and a wrong unread badge is the single
        // most-noticed bug a chat product can ship. Per-channel because the
        // cutoff differs per membership row, which is the one thing here that
        // does not fold into a GROUP BY.
        let mut unread_q = chat_messages::Entity::find()
            .filter(chat_messages::Column::ChannelId.eq(ch.id))
            .filter(chat_messages::Column::DeletedAt.is_null());
        if let Some(at) = m.last_read_at {
            unread_q = unread_q.filter(chat_messages::Column::CreatedAt.gt(at));
        }
        let unread = unread_q
            .count(&db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        out.push(ChannelSummary {
            id: ch.id,
            kind: ch.kind.clone(),
            name: ch.name.clone(),
            topic: ch.topic.clone(),
            archived: ch.archived_at.is_some(),
            members: member_counts.get(&ch.id).copied().unwrap_or(0),
            unread,
            last_message_at: last_message_at.get(&ch.id).map(|t| iso(*t)),
        });
    }

    // Busiest first — a channel list is scanned for "what needs me", and
    // alphabetical order answers a question nobody asked.
    out.sort_by(|a, b| {
        b.unread
            .cmp(&a.unread)
            .then(b.last_message_at.cmp(&a.last_message_at))
    });
    Ok(Json(out))
}

/// `GET /api/chat/channels/{id}/messages` — one page, newest first.
#[instrument(skip_all, fields(channel_id = %channel_id))]
pub async fn list_messages(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(channel_id): Path<Uuid>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<Json<Vec<MessageDto>>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if member_channel(&db, channel_id, user.id).await.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut query = chat_messages::Entity::find()
        .filter(chat_messages::Column::ChannelId.eq(channel_id))
        .order_by_desc(chat_messages::Column::CreatedAt)
        .order_by_desc(chat_messages::Column::Id)
        .limit(clamp_limit(q.limit));

    // Keyset, not OFFSET. A conversation grows at the end the reader is paging
    // away from, and OFFSET silently repeats rows when it does.
    //
    // The cursor has to carry the id as well as the timestamp, because the sort
    // is `(created_at DESC, id DESC)`. With `created_at < ts` alone, two
    // messages sharing an instant that straddle a page boundary lose one
    // permanently — the migration comment already said the `id DESC` tiebreak
    // "is what makes keyset paging total"; this is the half that was missing.
    if let Some(before) = q.before.as_deref() {
        // A cursor we cannot parse is a bug in the caller, not a reason to hand
        // back page 1 forever — which is what silently ignoring it did.
        let ts = DateTime::parse_from_rfc3339(before).map_err(|_| StatusCode::BAD_REQUEST)?;
        query = match q.before_id {
            Some(id) => query.filter(
                Condition::any()
                    .add(chat_messages::Column::CreatedAt.lt(ts))
                    .add(
                        Condition::all()
                            .add(chat_messages::Column::CreatedAt.eq(ts))
                            .add(chat_messages::Column::Id.lt(id)),
                    ),
            ),
            None => query.filter(chat_messages::Column::CreatedAt.lt(ts)),
        };
    }

    let rows = query
        .all(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // One lookup for the page, not one per message. A conversation is a handful
    // of people saying many things, so the per-row shape re-fetched the same
    // author dozens of times per page.
    let mut author_ids: Vec<Uuid> = rows.iter().filter_map(|m| m.author_id).collect();
    author_ids.sort_unstable();
    author_ids.dedup();
    let authors: HashMap<Uuid, String> = if author_ids.is_empty() {
        HashMap::new()
    } else {
        users::Entity::find()
            .filter(users::Column::Id.is_in(author_ids))
            .all(&db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(|u| (u.id, u.name))
            .collect()
    };

    let mut out = Vec::with_capacity(rows.len());
    for m in rows {
        let author_name = m.author_id.and_then(|id| authors.get(&id).cloned());
        out.push(MessageDto {
            id: m.id,
            channel_id: m.channel_id,
            author_id: m.author_id,
            author_name,
            // `visible_body` rather than `body`: a tombstone keeps its slot in
            // the thread and must never leak the text it replaced.
            body: m.visible_body().to_string(),
            created_at: iso(m.created_at),
            edited: m.edited_at.is_some(),
            deleted: m.deleted_at.is_some(),
        });
    }
    Ok(Json(out))
}

/// `POST /api/chat/channels/{id}/messages`
#[instrument(skip_all, fields(channel_id = %channel_id))]
pub async fn post_message(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(channel_id): Path<Uuid>,
    Json(body): Json<PostMessageRequest>,
) -> Result<(StatusCode, Json<MessageDto>), StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let Some(channel) = member_channel(&db, channel_id, user.id).await else {
        return Err(StatusCode::NOT_FOUND);
    };
    if !channel.is_writable() {
        return Err(StatusCode::CONFLICT);
    }

    let text = body.body.trim();
    if text.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    // A cap, because an uncapped body is an unbounded row and an unbounded
    // render. 8 KiB is far past any real message and far short of a problem.
    if text.len() > 8192 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let saved = chat_messages::ActiveModel {
        id: Set(Uuid::new_v4()),
        channel_id: Set(channel_id),
        author_id: Set(Some(user.id)),
        body: Set(text.to_string()),
        // NotSet, so the column's `DEFAULT now()` applies. Written from this
        // replica's clock it would be compared against a `last_read_at` written
        // from another's, and ordinary NTP skew is enough to leave a message
        // permanently unread — or to mark one read before it arrived. One clock,
        // the database's, is what makes unread mean anything.
        created_at: sea_orm::ActiveValue::NotSet,
        edited_at: Set(None),
        deleted_at: Set(None),
    }
    .insert(&db)
    .await
    .map_err(|e| {
        warn!(error = %e, "chat message insert failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // AFTER the commit, never before. Announcing first would wake readers who
    // then query and find nothing, which reads as a message that flickered and
    // vanished.
    delivery::announce(&db, channel_id).await;

    info!(%channel_id, author = %user.id, "chat message posted");
    Ok((
        StatusCode::CREATED,
        Json(MessageDto {
            id: saved.id,
            channel_id,
            author_id: Some(user.id),
            author_name: Some(user.name.clone()),
            body: saved.body,
            created_at: iso(saved.created_at),
            edited: false,
            deleted: false,
        }),
    ))
}

/// `POST /api/chat/channels/{id}/read` — mark read up to now.
#[instrument(skip_all, fields(channel_id = %channel_id))]
pub async fn mark_read(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(channel_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // Through `member_channel` like every other handler: this one used to load
    // the membership row directly, which skipped the org-standing check and made
    // it the one door a removed member could still walk through.
    if member_channel(&db, channel_id, user.id).await.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // `now()` from the DATABASE, not this replica's clock. Unread is a
    // comparison between `last_read_at` and `created_at`, and those are written
    // by different replicas — under ordinary NTP skew a message posted on A just
    // before a read on B lands with `created_at` after `last_read_at` and stays
    // unread forever. One clock is the only way the comparison is meaningful.
    chat_channel_members::Entity::update_many()
        .col_expr(
            chat_channel_members::Column::LastReadAt,
            sea_orm::sea_query::Expr::cust("now()"),
        )
        .filter(chat_channel_members::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_members::Column::UserId.eq(user.id))
        .exec(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/chat/channels/{id}/stream` — live delivery.
///
/// The stream carries **wake signals, not messages**: `NOTIFY` is
/// fire-and-forget, so a payload dropped while a replica reconnects is gone and
/// nothing replays it. A client that misses a wake re-reads the channel and is
/// whole again; a client that missed a *message* would have a permanent hole in
/// somebody's conversation.
#[instrument(skip_all, fields(channel_id = %channel_id))]
pub async fn stream(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(channel_id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let db = establish_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if member_channel(&db, channel_id, user.id).await.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut rx = delivery::subscribe(channel_id);
    let stream = async_stream::stream! {
        // An immediate hello, so a client knows the stream is live rather than
        // merely accepted — an SSE that opens and stays silent is
        // indistinguishable from one that is broken.
        yield Ok(Event::default().event("open").data("{}"));
        loop {
            match rx.recv().await {
                Ok(()) => yield Ok(Event::default().event("message").data("{}")),
                // Lagged: this subscriber fell behind a burst. Since the signal
                // carries nothing, one wake recovers everything it missed.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(%channel_id, skipped = n, "chat stream lagged; coalescing");
                    yield Ok(Event::default().event("message").data("{}"));
                }
                // A terminal event, per `crates/app/CLAUDE.md`. A chat stream is
                // open-ended rather than run-shaped, so this fires only when the
                // sender is dropped — but a client that can tell "the server
                // closed this" from "the network went away" retries correctly,
                // and the event costs one line.
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    yield Ok(Event::default().event("closed").data("{}"));
                    return;
                }
            }
        }
    };

    // A keep-alive is not optional here: proxies close an idle connection, and
    // a chat channel is idle most of the time.
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
