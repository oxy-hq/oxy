//! Writing a platform notice: the send, a signal per new recipient, and the
//! delivery task, in one transaction.
//!
//! **At most once per (recipient, org, kind, subject).** The check and the
//! insert run under a transaction-scoped advisory lock on (org, kind,
//! subject), so two pods observing the same workspace become ready, or a
//! webhook delivered twice, cannot both tell the same person. The org is part
//! of the key because not every subject id is unique across orgs: billing's is
//! the grace period's end, which two orgs can share to the second, and one
//! person can own both. A lock rather than a unique index
//! because `notifications` already exists and the previous deploy still
//! writes it: adding a constraint there is a contraction
//! (`migration_rollback_safety.rs`).
//!
//! **Nothing to render is stored.** The signal holds the kind and the
//! subject; `render` builds the words when the inbox is read or a channel
//! delivers. So unlike an app's notification this needs no org OLTP store.

use chrono::Utc;
use entity::{notification_sends, notifications, workspaces};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionTrait,
};
use uuid::Uuid;

use super::kinds::PlatformNotice;
use crate::server::api::notifications::task;

/// What one platform send did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSent {
    /// The send's id; `None` when every recipient had already been told.
    pub id: Option<Uuid>,
    /// People told now.
    pub recipients: usize,
    /// People who already held this notice.
    pub already_told: usize,
}

/// Tell `recipients` about `notice`, skipping anyone already told.
///
/// Fails only on Oxy's own database. Delivery is queued in the same
/// transaction under the notice's workspace (or the org's oldest one); an org
/// with no workspace at all delivers inline, best-effort, after the commit.
pub async fn notify(
    db: &DatabaseConnection,
    notice: &PlatformNotice,
    recipients: &[Uuid],
) -> Result<PlatformSent, DbErr> {
    let mut people: Vec<Uuid> = Vec::with_capacity(recipients.len());
    for r in recipients {
        if !people.contains(r) {
            people.push(*r);
        }
    }
    if people.is_empty() {
        return Ok(PlatformSent {
            id: None,
            recipients: 0,
            already_told: 0,
        });
    }
    let workspace = match notice.workspace_id() {
        Some(w) => Some(w),
        None => oldest_workspace(db, notice.org_id()).await?,
    };
    let txn = db.begin().await?;
    lock_subject(&txn, notice).await?;
    let told = already_told(&txn, notice, &people).await?;
    let fresh: Vec<Uuid> = people
        .iter()
        .copied()
        .filter(|u| !told.contains(u))
        .collect();
    if fresh.is_empty() {
        txn.commit().await?;
        return Ok(PlatformSent {
            id: None,
            recipients: 0,
            already_told: people.len(),
        });
    }
    let id = Uuid::new_v4();
    let run_id = workspace.map(|_| Uuid::new_v4().to_string());
    record(&txn, id, notice, &fresh, run_id.clone()).await?;
    if let (Some(workspace_id), Some(run_id)) = (workspace, run_id.as_deref()) {
        task::enqueue(&txn, run_id, id, workspace_id).await?;
    }
    txn.commit().await?;
    if workspace.is_none() {
        let report = crate::server::api::notifications::delivery::deliver_send(db, id).await;
        tracing::info!(send = %id, ?report, "platform notice delivered inline: the org has no workspace");
    }
    tracing::info!(
        kind = notice.kind(),
        send = %id,
        recipients = fresh.len(),
        already_told = told.len(),
        "platform notice sent"
    );
    Ok(PlatformSent {
        id: Some(id),
        recipients: fresh.len(),
        already_told: told.len(),
    })
}

/// Serialize every send about this (org, kind, subject) until the
/// transaction ends. Keyed on a 64-bit hash: a collision only makes two unrelated sends
/// wait for each other.
async fn lock_subject(txn: &DatabaseTransaction, notice: &PlatformNotice) -> Result<(), DbErr> {
    let subject = notice.subject();
    let key = format!(
        "notice|{}|{}|{}|{}",
        notice.org_id(),
        notice.kind(),
        subject.kind,
        subject.id
    );
    txn.execute_raw(Statement::from_sql_and_values(
        txn.get_database_backend(),
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        [key.into()],
    ))
    .await?;
    Ok(())
}

/// Which of `people` already hold this notice, in this org.
async fn already_told(
    txn: &DatabaseTransaction,
    notice: &PlatformNotice,
    people: &[Uuid],
) -> Result<Vec<Uuid>, DbErr> {
    let subject = notice.subject();
    notifications::Entity::find()
        .select_only()
        .column(notifications::Column::UserId)
        .filter(notifications::Column::UserId.is_in(people.iter().copied()))
        .filter(notifications::Column::OrgId.eq(notice.org_id()))
        .filter(notifications::Column::Kind.eq(notice.kind()))
        .filter(notifications::Column::SubjectKind.eq(subject.kind))
        .filter(notifications::Column::SubjectId.eq(subject.id))
        .into_tuple::<Uuid>()
        .all(txn)
        .await
}

async fn record(
    txn: &DatabaseTransaction,
    id: Uuid,
    notice: &PlatformNotice,
    people: &[Uuid],
    run_id: Option<String>,
) -> Result<(), DbErr> {
    let now = Utc::now().fixed_offset();
    let subject = notice.subject();
    notification_sends::ActiveModel {
        id: Set(id),
        org_id: Set(notice.org_id()),
        app_id: Set(None),
        environment: Set("production".to_string()),
        kind: Set(notice.kind().to_string()),
        critical: Set(notice.critical()),
        email: Set(notice.emailed()),
        slack: Set(notice.slacked()),
        idempotency_key_sha256: Set(None),
        recipients: Set(i32::try_from(people.len()).unwrap_or(i32::MAX)),
        run_id: Set(run_id),
        created_at: Set(now),
    }
    .insert(txn)
    .await?;
    notifications::Entity::insert_many(people.iter().map(|user| notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        org_id: Set(notice.org_id()),
        user_id: Set(*user),
        kind: Set(notice.kind().to_string()),
        // Empty, not NULL, for the previous deploy's decoder (see `send.rs`).
        title: Set(String::new()),
        body: Set(None),
        link: Set(None),
        subject_kind: Set(Some(subject.kind.to_string())),
        subject_id: Set(Some(subject.id.clone())),
        created_at: Set(now),
        read_at: Set(None),
        // The send, so delivery finds its signals the way it finds an app's.
        message_id: Set(Some(id)),
        app_id: Set(None),
    }))
    .exec(txn)
    .await?;
    Ok(())
}

/// The org's oldest workspace, to record an org-level notice's delivery run
/// under: the run table needs a workspace and an org-level notice has none.
async fn oldest_workspace(db: &DatabaseConnection, org_id: Uuid) -> Result<Option<Uuid>, DbErr> {
    workspaces::Entity::find()
        .select_only()
        .column(workspaces::Column::Id)
        .filter(workspaces::Column::OrgId.eq(org_id))
        .order_by_asc(workspaces::Column::CreatedAt)
        .into_tuple::<Uuid>()
        .one(db)
        .await
}
