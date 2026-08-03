//! The app-access control plane, independent of who is asking.
//!
//! Three surfaces edit the same thing — the org's own settings, the Oxy admin
//! panel, and the partner console — and they authenticate in three different ways
//! that cannot be unified: org routes need a real membership or a live assume-role
//! session, `/admin/*` is closed while an operator is acting, and the partner
//! console is capability-scoped. So the AUTHORITY differs per surface, but the
//! BEHAVIOR must not. Everything below is that shared behavior; each surface
//! contributes only its own gate and a thin handler.
//!
//! Nothing here decides access — callers gate first, then call.

use axum::http::StatusCode;
use entity::prelude::{
    AppMembers, AppTeamGrants, Apps, OrgMembers, OrgTeamMembers, OrgTeams, Users,
};
use entity::{app_members, app_team_grants, apps, org_members, org_team_members, org_teams, users};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use std::collections::HashMap;
use uuid::Uuid;

use super::dto::{
    AppAccessDto, AppAccessSummaryDto, GrantDto, GranteeRef, OrgMemberOptionDto,
    SetAppAccessRequest, TeamDto,
};

/// The two values `apps.visibility` accepts. A DB CHECK enforces the same set —
/// this rejects at the edge so a typo is a 400, not a 500.
pub const VISIBILITY_ORG: &str = "org";
pub const VISIBILITY_MEMBERS: &str = "members";

/// Backstop on one app's grant list, so a client can't write an unbounded roster.
const MAX_GRANTS_PER_APP: usize = 200;

pub fn db_err(e: impl std::fmt::Display) -> StatusCode {
    tracing::error!("app_access: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

/// Load an app, 404ing when it belongs to another org — the tenant boundary. Every
/// surface passes the org it believes owns the app, so a mismatched id can never
/// resolve across tenants regardless of how the caller was authorized.
pub async fn load_app_in_org(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_id: Uuid,
) -> Result<apps::Model, StatusCode> {
    Apps::find_by_id(app_id)
        .filter(apps::Column::OrgId.eq(org_id))
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or(StatusCode::NOT_FOUND)
}

// ── Read ────────────────────────────────────────────────────────────────────

/// An app's current visibility and full grant list.
pub async fn read_access(
    db: &DatabaseConnection,
    app: &apps::Model,
) -> Result<AppAccessDto, StatusCode> {
    let mut grants = team_grants(db, app.id).await?;
    grants.extend(user_grants(db, app.id).await?);
    Ok(AppAccessDto {
        app_id: app.id,
        visibility: app.visibility.clone(),
        grants,
    })
}

/// Team grants, with the headcount each one actually reaches — an admin about to
/// hand `admin` to a 40-person team should see the 40 before they save.
async fn team_grants(db: &DatabaseConnection, app_id: Uuid) -> Result<Vec<GrantDto>, StatusCode> {
    let rows = AppTeamGrants::find()
        .filter(app_team_grants::Column::AppId.eq(app_id))
        .find_also_related(OrgTeams)
        .all(db)
        .await
        .map_err(db_err)?;

    let counts = team_member_counts(db, rows.iter().map(|(g, _)| g.team_id).collect()).await?;
    let mut out: Vec<GrantDto> = rows
        .into_iter()
        .filter_map(|(grant, team)| {
            let team: org_teams::Model = team?;
            Some(GrantDto {
                kind: "team",
                member_count: Some(counts.get(&team.id).copied().unwrap_or(0)),
                id: team.id,
                name: team.name,
                email: None,
                role: grant.role,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

async fn user_grants(db: &DatabaseConnection, app_id: Uuid) -> Result<Vec<GrantDto>, StatusCode> {
    let rows = AppMembers::find()
        .filter(app_members::Column::AppId.eq(app_id))
        .find_also_related(Users)
        .order_by_asc(app_members::Column::CreatedAt)
        .all(db)
        .await
        .map_err(db_err)?;

    let mut out: Vec<GrantDto> = rows
        .into_iter()
        .filter_map(|(grant, user)| {
            let user: users::Model = user?;
            Some(GrantDto {
                kind: "user",
                member_count: None,
                id: user.id,
                name: display_name(&user),
                email: Some(user.email),
                role: grant.role,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn display_name(user: &users::Model) -> String {
    if user.name.trim().is_empty() {
        user.email.clone()
    } else {
        user.name.clone()
    }
}

/// Member counts for many teams in ONE query — every surface that lists teams would
/// otherwise be an N+1 over the org's whole roster.
pub async fn team_member_counts(
    db: &DatabaseConnection,
    team_ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, u64>, StatusCode> {
    if team_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut counts: HashMap<Uuid, u64> = HashMap::new();
    for row in OrgTeamMembers::find()
        .filter(org_team_members::Column::TeamId.is_in(team_ids))
        .all(db)
        .await
        .map_err(db_err)?
    {
        *counts.entry(row.team_id).or_default() += 1;
    }
    Ok(counts)
}

/// Every team in an org, with member counts — the grant picker's source list.
pub async fn list_org_teams(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<Vec<TeamDto>, StatusCode> {
    let teams = OrgTeams::find()
        .filter(org_teams::Column::OrgId.eq(org_id))
        .order_by_asc(org_teams::Column::Name)
        .all(db)
        .await
        .map_err(db_err)?;
    let counts = team_member_counts(db, teams.iter().map(|t| t.id).collect()).await?;
    Ok(teams.into_iter().map(|t| to_team_dto(t, &counts)).collect())
}

/// Every app in an org with its visibility and grant count — the org's
/// "who can open what" list.
///
/// Grant counts for the whole list come from two batched queries, not one per app:
/// an org with thirty apps would otherwise fan out to sixty round trips to render
/// a settings page.
pub async fn list_org_apps_with_access(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<Vec<AppAccessSummaryDto>, StatusCode> {
    let rows = Apps::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .order_by_asc(apps::Column::Name)
        .all(db)
        .await
        .map_err(db_err)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let app_ids: Vec<Uuid> = rows.iter().map(|a| a.id).collect();
    let mut counts: HashMap<Uuid, u64> = HashMap::new();
    for m in AppMembers::find()
        .filter(app_members::Column::AppId.is_in(app_ids.clone()))
        .all(db)
        .await
        .map_err(db_err)?
    {
        *counts.entry(m.app_id).or_default() += 1;
    }
    for g in AppTeamGrants::find()
        .filter(app_team_grants::Column::AppId.is_in(app_ids))
        .all(db)
        .await
        .map_err(db_err)?
    {
        *counts.entry(g.app_id).or_default() += 1;
    }

    Ok(rows
        .into_iter()
        .map(|a| AppAccessSummaryDto {
            grant_count: counts.get(&a.id).copied().unwrap_or(0),
            published: a.published_at.is_some(),
            id: a.id,
            name: a.name,
            slug: a.slug,
            visibility: a.visibility,
        })
        .collect())
}

/// Everyone in an org, for the "add a person" picker. Same shape the partner
/// console already returns from its own member list, so one frontend component
/// feeds from either.
pub async fn list_org_member_options(
    db: &DatabaseConnection,
    org_id: Uuid,
) -> Result<Vec<OrgMemberOptionDto>, StatusCode> {
    let rows = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .find_also_related(Users)
        .all(db)
        .await
        .map_err(db_err)?;
    let mut out: Vec<OrgMemberOptionDto> = rows
        .into_iter()
        .filter_map(|(m, user)| {
            let user: users::Model = user?;
            Some(OrgMemberOptionDto {
                user_id: m.user_id,
                name: display_name(&user),
                email: user.email,
                role: m.role.as_str().to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn to_team_dto(t: org_teams::Model, counts: &HashMap<Uuid, u64>) -> TeamDto {
    TeamDto {
        member_count: counts.get(&t.id).copied().unwrap_or(0),
        id: t.id,
        name: t.name,
        description: t.description,
        created_at: t.created_at.to_rfc3339(),
    }
}

// ── Write ───────────────────────────────────────────────────────────────────

/// Replace an app's visibility and whole grant list in one transaction.
///
/// `app` must be a **freshly loaded** row — every caller goes through
/// [`load_app_in_org`] immediately before this. Two things read off it are wrong if
/// it's stale: `org_id` scopes the grantee/team validation, and `id` keys the
/// writes. Passing a cached copy would validate against the wrong tenant.
///
/// `visibility` is deliberately NOT one of them: the update filters on the committed
/// value in SQL, so a stale copy can't swallow a change (see the `WHERE` clause
/// below, and `a_stale_row_cannot_swallow_a_visibility_change`).
///
/// A full replace rather than incremental add/remove: the UI edits one list and
/// saves it, and a replace has no interleaving window between two admins editing the
/// same app — the second save wins wholesale instead of merging halfway.
///
/// Validates before it writes, and drops the access cache after it commits.
pub async fn write_access(
    db: &DatabaseConnection,
    app: &apps::Model,
    actor_id: Uuid,
    req: &SetAppAccessRequest,
) -> Result<AppAccessDto, StatusCode> {
    let visibility = match req.visibility.as_str() {
        VISIBILITY_ORG => VISIBILITY_ORG,
        VISIBILITY_MEMBERS => VISIBILITY_MEMBERS,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    if req.grants.len() > MAX_GRANTS_PER_APP {
        return Err(StatusCode::BAD_REQUEST);
    }
    for grant in &req.grants {
        if !matches!(
            grant.role(),
            app_members::ROLE_ADMIN | app_members::ROLE_MEMBER
        ) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Collapse repeats FIRST, and write from the collapsed list — not just from its
    // ids. A payload naming the same grantee twice (a script, or a retried request
    // the UI can't produce) would otherwise trip `app_members_app_user_unique`
    // mid-transaction and surface as a 500 with an ERROR log, when it is a
    // well-formed request with a redundant entry. Last-wins matches the full-replace
    // semantics of the endpoint: the final mention is the intended role.
    let grants = dedupe_grantees(&req.grants);

    let (user_ids, team_ids) = split_grantees(&grants);
    // Both boundary checks happen BEFORE any write: a grantee must be a member of
    // this org, and a team must belong to it. `Ring::AppAccess` enforces the
    // membership half independently, but rejecting here is what makes the rule
    // visible to the admin instead of silently writing a grant that grants nothing.
    validate_users_are_org_members(db, app.org_id, &user_ids).await?;
    validate_teams_belong_to_org(db, app.org_id, &team_ids).await?;

    let txn = db.begin().await.map_err(db_err)?;
    // Touch the row only when `visibility` ACTUALLY changed — but let POSTGRES make
    // that comparison, in the `WHERE`, not Rust against `app`.
    //
    // `app` was loaded before `BEGIN`, so comparing against it decides the write
    // from a pre-transaction snapshot. Two saves racing on one app:
    //
    //   A and B both load it at `org`.
    //   A saves `members` → its snapshot says `org`, so it writes. Committed:
    //   `members`. B saves `org` → its snapshot ALSO says `org`, so it skips — and
    //   the row stays `members` while B's grants replace A's.
    //
    // B asked for "everyone in the organization" and got "only people you choose":
    // grants from B, visibility from A, which is precisely the halfway merge the
    // full-replace contract above promises can't happen. It's the one axis where
    // losing TIGHTENS access, so the symptom isn't an error — it's "my app vanished
    // from members' launchers after I opened it up".
    //
    // Filtering on the committed value is also what makes the post-commit re-read at
    // the end meaningful: the write lands whenever the stored value differs, so the
    // row that read returns actually reflects this request.
    //
    // Still writes nothing (and takes no row lock) when the value already matches,
    // so a genuine no-op save leaves `updated_at` alone and the admin apps list
    // stops reordering for changes that didn't happen. Grants live in their own
    // tables, so a grant-only edit correctly leaves this row untouched.
    Apps::update_many()
        .col_expr(apps::Column::Visibility, Expr::value(visibility))
        // `CURRENT_TIMESTAMP`, not the application clock. The admin apps list sorts
        // `updated_at DESC`, and stamping from the process means two replicas with
        // skewed clocks can order each other's writes wrongly. Free to get right
        // here; the surrounding code stamps from Rust only because that's what the
        // `ActiveModel` idiom does.
        .col_expr(apps::Column::UpdatedAt, Expr::current_timestamp().into())
        .filter(apps::Column::Id.eq(app.id))
        .filter(apps::Column::Visibility.ne(visibility))
        .exec(&txn)
        .await
        .map_err(db_err)?;

    AppMembers::delete_many()
        .filter(app_members::Column::AppId.eq(app.id))
        .exec(&txn)
        .await
        .map_err(db_err)?;
    AppTeamGrants::delete_many()
        .filter(app_team_grants::Column::AppId.eq(app.id))
        .exec(&txn)
        .await
        .map_err(db_err)?;

    // Two statements rather than up to 200 inside the transaction — the cap allows a
    // 200-grant list, and a round trip each would hold the row locks that long.
    let (user_rows, team_rows): (Vec<_>, Vec<_>) =
        grants.iter().fold((vec![], vec![]), |mut acc, grant| {
            match grant {
                GranteeRef::User { id, role } => acc.0.push(app_members::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    app_id: ActiveValue::Set(app.id),
                    user_id: ActiveValue::Set(*id),
                    role: ActiveValue::Set(role.clone()),
                    created_by: ActiveValue::Set(Some(actor_id)),
                    created_at: ActiveValue::NotSet,
                }),
                GranteeRef::Team { id, role } => acc.1.push(app_team_grants::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    app_id: ActiveValue::Set(app.id),
                    team_id: ActiveValue::Set(*id),
                    role: ActiveValue::Set(role.clone()),
                    created_by: ActiveValue::Set(Some(actor_id)),
                    created_at: ActiveValue::NotSet,
                }),
            }
            acc
        });
    if !user_rows.is_empty() {
        AppMembers::insert_many(user_rows)
            .exec(&txn)
            .await
            .map_err(db_err)?;
    }
    if !team_rows.is_empty() {
        AppTeamGrants::insert_many(team_rows)
            .exec(&txn)
            .await
            .map_err(db_err)?;
    }
    txn.commit().await.map_err(db_err)?;

    // The `(user_id, app_id)` access cache has a TTL, so without this a revoke keeps
    // working for up to a minute on THIS replica.
    //
    // The cache is per-process: on a multi-replica serve fleet only the replica that
    // took the write drops it, so other replicas keep honoring a revoked grant until
    // their own entry ages out (`CACHE_TTL`, 60s — see `custom_apps_cache`). That
    // bound is accepted, not closed: a cross-replica invalidation would need a
    // broadcast channel, and 60s of stale ALLOW on a revoke is within what the rest
    // of the membership caches already permit. Say so plainly rather than implying
    // the call below is a fleet-wide flush.
    crate::server::api::custom_apps_auth::invalidate_access_cache();
    tracing::info!(
        app = %app.id, org = %app.org_id, actor = %actor_id,
        visibility, grants = grants.len(),
        "app access updated"
    );

    // Re-READ the row rather than asserting what we asked for. Setting
    // `updated.visibility = visibility` would report this caller's request even if a
    // concurrent save had since changed it — pairing our visibility with the other
    // save's grants, which is the same halfway-merge shape the `WHERE` clause above
    // rules out for the row itself, just in the payload the client caches. The
    // grants are still read post-commit and so are equally a snapshot; what this
    // buys is that the two halves of the response now come from the same read
    // instead of one being a claim.
    //
    // If the row vanished (deleted between commit and re-read), report the requested
    // value directly — the write did happen, so that's truer than 404ing, and
    // querying grants for an id whose rows have just cascaded away would be three
    // round trips to build an empty list.
    match Apps::find_by_id(app.id).one(db).await.map_err(db_err)? {
        Some(row) => read_access(db, &row).await,
        None => Ok(AppAccessDto {
            app_id: app.id,
            visibility: visibility.to_string(),
            grants: vec![],
        }),
    }
}

/// Collapse repeated grantees, **last mention wins**.
///
/// A repeat is a well-formed request with a redundant entry, not an error: the
/// endpoint replaces the whole list, so the last mention is the intended role. The
/// UI can't produce one (its `addGrant` is idempotent), but a script or a retried
/// request can — and without this the second row trips the unique index
/// mid-transaction and the caller gets a 500 for a request we understood perfectly.
///
/// Kind is part of the identity: a user and a team may share a UUID (different
/// tables), so collapsing on id alone would drop a legitimate grant.
pub fn dedupe_grantees(grants: &[GranteeRef]) -> Vec<GranteeRef> {
    let mut out: Vec<GranteeRef> = Vec::with_capacity(grants.len());
    for grant in grants {
        let (kind_is_user, id) = match grant {
            GranteeRef::User { id, .. } => (true, *id),
            GranteeRef::Team { id, .. } => (false, *id),
        };
        let existing = out.iter().position(|g| match g {
            GranteeRef::User { id: other, .. } => kind_is_user && *other == id,
            GranteeRef::Team { id: other, .. } => !kind_is_user && *other == id,
        });
        match existing {
            Some(i) => out[i] = grant.clone(),
            None => out.push(grant.clone()),
        }
    }
    out
}

/// Partition grantees by kind. Expects an already-deduped list — see
/// [`dedupe_grantees`], which the write path runs first.
pub fn split_grantees(grants: &[GranteeRef]) -> (Vec<Uuid>, Vec<Uuid>) {
    let mut users = Vec::new();
    let mut teams = Vec::new();
    for g in grants {
        match g {
            GranteeRef::User { id, .. } if !users.contains(id) => users.push(*id),
            GranteeRef::Team { id, .. } if !teams.contains(id) => teams.push(*id),
            _ => {}
        }
    }
    (users, teams)
}

async fn validate_users_are_org_members(
    db: &DatabaseConnection,
    org_id: Uuid,
    user_ids: &[Uuid],
) -> Result<(), StatusCode> {
    if user_ids.is_empty() {
        return Ok(());
    }
    let found = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.is_in(user_ids.to_vec()))
        .all(db)
        .await
        .map_err(db_err)?
        .len();
    if found == user_ids.len() {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

async fn validate_teams_belong_to_org(
    db: &DatabaseConnection,
    org_id: Uuid,
    team_ids: &[Uuid],
) -> Result<(), StatusCode> {
    if team_ids.is_empty() {
        return Ok(());
    }
    let found = OrgTeams::find()
        .filter(org_teams::Column::OrgId.eq(org_id))
        .filter(org_teams::Column::Id.is_in(team_ids.to_vec()))
        .all(db)
        .await
        .map_err(db_err)?
        .len();
    if found == team_ids.len() {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Roles must survive the collapse with **last mention winning** — the property
    /// the 500-fix turns on, and one `split_grantees` can't see because it only
    /// returns ids. Flipping this to first-wins would silently write the wrong role.
    #[test]
    fn dedupe_keeps_the_last_mention_of_a_grantee() {
        let u = Uuid::from_u128(1);
        let t = Uuid::from_u128(2);
        let out = dedupe_grantees(&[
            GranteeRef::User {
                id: u,
                role: "member".into(),
            },
            GranteeRef::Team {
                id: t,
                role: "admin".into(),
            },
            GranteeRef::User {
                id: u,
                role: "admin".into(),
            },
            GranteeRef::Team {
                id: t,
                role: "member".into(),
            },
        ]);
        assert_eq!(out.len(), 2, "each grantee collapses to one entry");
        // The user was member-then-admin, so admin; the team was admin-then-member,
        // so member. Asserting BOTH directions is what rules out first-wins.
        assert!(matches!(
            &out[0],
            GranteeRef::User { id, role } if *id == u && role == "admin"
        ));
        assert!(matches!(
            &out[1],
            GranteeRef::Team { id, role } if *id == t && role == "member"
        ));
    }

    #[test]
    fn dedupe_preserves_first_appearance_order() {
        // Order is the list the admin sees and the order rows are written in; a
        // repeat should update in place, not move the entry to the end.
        let a = Uuid::from_u128(10);
        let b = Uuid::from_u128(11);
        let out = dedupe_grantees(&[
            GranteeRef::Team {
                id: a,
                role: "member".into(),
            },
            GranteeRef::Team {
                id: b,
                role: "member".into(),
            },
            GranteeRef::Team {
                id: a,
                role: "admin".into(),
            },
        ]);
        assert!(matches!(&out[0], GranteeRef::Team { id, .. } if *id == a));
        assert!(matches!(&out[1], GranteeRef::Team { id, .. } if *id == b));
    }

    #[test]
    fn dedupe_keeps_a_user_and_a_team_that_share_an_id() {
        // Different tables, so a shared UUID is legal — collapsing on id alone would
        // drop a real grant.
        let id = Uuid::from_u128(7);
        let out = dedupe_grantees(&[
            GranteeRef::User {
                id,
                role: "member".into(),
            },
            GranteeRef::Team {
                id,
                role: "admin".into(),
            },
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn grantees_split_by_kind_and_dedupe() {
        let u = Uuid::from_u128(1);
        let t = Uuid::from_u128(2);
        let grants = vec![
            GranteeRef::User {
                id: u,
                role: "member".into(),
            },
            GranteeRef::Team {
                id: t,
                role: "admin".into(),
            },
            // A repeat would trip the unique index mid-transaction.
            GranteeRef::User {
                id: u,
                role: "admin".into(),
            },
        ];
        let (users, teams) = split_grantees(&grants);
        assert_eq!(users, vec![u]);
        assert_eq!(teams, vec![t]);
    }

    #[test]
    fn empty_grant_list_splits_cleanly() {
        let (users, teams) = split_grantees(&[]);
        assert!(users.is_empty() && teams.is_empty());
    }

    #[test]
    fn a_team_and_a_user_sharing_an_id_are_kept_apart() {
        // The two grant kinds live in different tables, so the same UUID appearing
        // as both is legal and must not collapse.
        let id = Uuid::from_u128(7);
        let (users, teams) = split_grantees(&[
            GranteeRef::User {
                id,
                role: "member".into(),
            },
            GranteeRef::Team {
                id,
                role: "member".into(),
            },
        ]);
        assert_eq!(users, vec![id]);
        assert_eq!(teams, vec![id]);
    }
}
