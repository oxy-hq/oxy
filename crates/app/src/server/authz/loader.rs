//! Turn a `(user_id, email)` into [`PrincipalFacts`] — the org sets, partner standings
//! and global flags [`oxy_authz::allows`] decides over. This is consolidation, not a new
//! authority; every fact comes from an existing primitive:
//!
//! * org roles    → one `org_members` query, with owner/admin/member derived from the
//!   rows in memory ([`derive_org_roles`])
//! * partner sets → bounded queries over active grants + partner access + managed
//!   clients, and their ceilings — straight from `partner_authz::operated_partners` /
//!   `assumed_partners`, the single source `resolve_scope` also reads. This once
//!   hand-rolled those queries under a "keep the two in sync" comment, which was the
//!   drift it exists to prevent; there is one implementation now.
//! * ws overrides → one `workspace_members` query, only the exceptional elevations
//! * global admin → [`is_app_admin_email`] (`app_admins` table)
//! * global owner → [`is_oxy_owner`] (`OXY_OWNER` env allow-list)
//!
//! Cost (design §5): the guards call this at most ONCE per request — `authz::
//! request_facts` memoizes it in the request's extensions. The partner queries
//! short-circuit to empty for the common non-partner user, so a typical request is
//! three queries: memberships + ws-overrides + app-admins.

use entity::org_members::OrgRole;
use entity::prelude::*;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use oxy_authz::{Cap, PartnerStanding, PrincipalFacts};

use crate::server::api::middlewares::partner_authz;
use crate::server::authz::globals;

/// Load the authorization-relevant facts for a principal. `None` means **"we don't
/// know"** — a lookup errored — and is deliberately not the same value as `Some(facts)`
/// with empty sets, which means "we asked, and this principal belongs to nothing".
///
/// ## Why this returns an Option
///
/// This used to collapse a failed query to an empty set and call that "fail closed,
/// so the worst case is more denial". That reasoning is right in isolation and wrong
/// here. Under `enforce`, empty sets are read as *facts*: the model concludes the
/// principal is in no org, denies, and `existing_allow && false` turns a transient blip
/// — a statement timeout, a serialization failure, a dropped socket — into a **403 for
/// a legitimate org officer on every guarded route** until it clears.
///
/// It also quietly broke the promise `server::authz` makes: "a database blip must not
/// lock every tenant out of their org". That fail-safe only ever covered a failure to
/// *establish* a connection (`request_facts` → `None`); a connection that is up but
/// errors on one lookup sailed straight past it and denied.
///
/// So the failure is surfaced instead of averaged into the data, and each caller says
/// what unknown means for it. There is no safe blanket answer: an `enforce` caller can
/// defer to `existing_allow` (the conjunction only ever subtracts, so deferring cannot
/// open a hole), but a caller reading `allows` with no legacy term beside it must treat
/// unknown as **deny** — see `thread.rs`, where unknown standing must never confer
/// operator reach.
pub async fn load_principal_facts(
    db: &DatabaseConnection,
    user_id: Uuid,
    email: &str,
) -> Option<PrincipalFacts> {
    load_principal_facts_scoped(db, user_id, email, true).await
}

/// As [`load_principal_facts`], but `include_workspace_facts = false` skips the
/// `workspace_members` query that only the WorkspaceAdmin ring reads.
///
/// This exists for the customer-app data plane, which enforces AppAccess on the query
/// hot path (`oxy-customer-apps-perf`): AppAccess reads member_orgs, is_global_admin
/// and develop_apps_orgs only, so paying for the workspace-override lookup on every
/// query would be pure waste. One function with a flag, rather than a second loader
/// that could drift from this one.
pub async fn load_principal_facts_scoped(
    db: &DatabaseConnection,
    user_id: Uuid,
    email: &str,
    include_workspace_facts: bool,
) -> Option<PrincipalFacts> {
    // One query for every org membership; the org sets and the partner check both read
    // from these rows instead of re-querying per membership.
    //
    // `Ok(vec![])` (belongs to nothing) and `Err(_)` (we couldn't ask) are different
    // facts and must not become the same empty Vec — conflating them is what turned a
    // blip into a 403.
    let memberships = OrgMembers::find()
        .filter(entity::org_members::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(|e| {
            tracing::warn!(
                target: "authz",
                error = %e,
                user = %user_id,
                "org membership lookup failed — facts are unknown, not empty"
            );
        })
        .ok()?;

    let (owned_orgs, admin_orgs, member_orgs) = derive_org_roles(&memberships);
    // Read the platform sources ONCE: the partner step needs the staff verdict to decide
    // whether to look for assume sessions at all, and the facts need the flags. This
    // runs on the customer-app query hot path.
    let standing = globals::platform_standing_checked(db, email).await?;
    // NOTE: partner standings still collapse a failed query to "no standing" inside
    // `partner_authz` (operated_partners / standings_for). Same class as the bug this
    // Option fixes — a blip can still cost a partner their reach — but surfacing it
    // means threading fallibility through `resolve_scope`, whose 404-vs-403 existence
    // hiding has to be decided rather than mechanically rewritten. So `Some` here means
    // the loader's OWN reads succeeded, not that every fact in it is known-good.
    let partners = load_partner_standings(db, &memberships, user_id, standing.is_staff()).await;
    let ws_admin_override = if include_workspace_facts {
        load_ws_admin_override(db, user_id).await?
    } else {
        Vec::new()
    };

    Some(PrincipalFacts {
        user_id,
        owned_orgs,
        admin_orgs,
        member_orgs,
        partners,
        ws_admin_override,
        is_global_admin: standing.is_global_admin,
        is_global_owner: standing.is_global_owner,
    })
}

/// Facts for a PLATFORM decision (Oxy's operator surfaces): the two global flags only.
///
/// The platform rings read nothing else — no org set can reach the `Platform`
/// singleton — so this skips every org / partner / workspace query. An admin route
/// must not pay for tenant facts it never consults. `is_app_admin_email` is itself
/// cached, so this is ~free.
pub async fn load_platform_facts(
    db: &DatabaseConnection,
    user_id: Uuid,
    email: &str,
) -> Option<PrincipalFacts> {
    let standing = globals::platform_standing_checked(db, email).await?;
    Some(PrincipalFacts {
        user_id,
        is_global_admin: standing.is_global_admin,
        is_global_owner: standing.is_global_owner,
        ..Default::default()
    })
}

/// `(owned, admin, member)` org sets from the user's already-loaded `org_members`
/// rows (no query). `owned ⊆ admin ⊆ member` (owner implies admin implies member) so
/// a single role check populates the ring the action needs.
fn derive_org_roles(
    memberships: &[entity::org_members::Model],
) -> (Vec<Uuid>, Vec<Uuid>, Vec<Uuid>) {
    let mut owned = Vec::new();
    let mut admin = Vec::new();
    let mut member = Vec::new();
    for m in memberships {
        member.push(m.org_id);
        if matches!(m.role, OrgRole::Owner | OrgRole::Admin) {
            admin.push(m.org_id);
        }
        if matches!(m.role, OrgRole::Owner) {
            owned.push(m.org_id);
        }
    }
    (owned, admin, member)
}

/// The principal's partner standings, straight from [`partner_authz::operated_partners`]
/// — the same source `resolve_scope` reads.
///
/// This used to hand-roll the operator + ceiling queries and carry a "keep these two in
/// sync" comment. That was the drift-between-siblings failure this whole layer exists to
/// remove, re-created by hand and left for a human to police. Now there is one
/// implementation; the loader just reshapes it into facts.
async fn load_partner_standings(
    db: &DatabaseConnection,
    memberships: &[entity::org_members::Model],
    user_id: Uuid,
    is_staff: bool,
) -> Vec<PartnerStanding> {
    // Real operators, plus any partner this user is standing in through a live
    // assume-role session. The assumed half is what the model used to be blind to — the
    // console's scope knew about the override and the facts didn't, so the unified rings
    // would have denied staff the console. That gap is why partner_policy could not
    // retire. Non-staff short-circuit before any query.
    let mut standings = partner_authz::operated_partners(db, memberships).await;
    let assumed = partner_authz::assumed_partners(db, user_id, is_staff).await;
    for a in assumed {
        // A real operator's standing wins; they are the same ceiling anyway.
        if !standings.iter().any(|p| p.partner_id == a.partner_id) {
            standings.push(a);
        }
    }
    standings
        .into_iter()
        .map(|p| PartnerStanding {
            partner_id: p.partner_id,
            client_orgs: p.managed_org_ids,
            caps: caps_of(&p.capabilities),
        })
        .collect()
}

/// Translate the partner tier's ceiling into the authz vocabulary. One mapping, and the
/// compiler sees every arm of it.
fn caps_of(c: &partner_authz::Capabilities) -> Vec<Cap> {
    let mut caps = Vec::new();
    for (on, cap) in [
        (c.manage_members, Cap::ManageMembers),
        (c.manage_apps, Cap::ManageApps),
        (c.develop_apps, Cap::DevelopApps),
        (c.view_audit, Cap::ViewAudit),
        (c.manage_billing, Cap::ManageBilling),
        (c.manage_secrets, Cap::ManageSecrets),
        (c.create_orgs, Cap::CreateOrgs),
        (c.manage_org_settings, Cap::ManageOrgSettings),
    ] {
        if on {
            caps.push(cap);
        }
    }
    caps
}

/// Workspaces where a `workspace_members` override raises the user to Admin or Owner.
/// Only these exceptional rows are loaded — the org-derived workspace role comes free
/// from the org sets via the hierarchy, and overrides can only elevate. One bounded
/// query.
///
/// `None` = the query errored. Same reasoning as the org sets: an elevated member whose
/// override lookup blips is not "a member with no override", and reporting them as one
/// costs them the workspace their `workspace_members` row grants.
async fn load_ws_admin_override(db: &DatabaseConnection, user_id: Uuid) -> Option<Vec<Uuid>> {
    let rows = WorkspaceMembers::find()
        .filter(entity::workspace_members::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(|e| {
            tracing::warn!(
                target: "authz",
                error = %e,
                user = %user_id,
                "workspace override lookup failed — facts are unknown, not empty"
            );
        })
        .ok()?;
    Some(
        rows.into_iter()
            .filter(|m| {
                matches!(
                    m.role,
                    entity::workspace_members::WorkspaceRole::Owner
                        | entity::workspace_members::WorkspaceRole::Admin
                )
            })
            .map(|m| m.workspace_id)
            .collect(),
    )
}
