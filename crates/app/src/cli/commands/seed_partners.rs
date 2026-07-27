//! Partner + multi-tenant seed data — realistic orgs, generated people,
//! partnerships and workspaces so a developer can immediately exercise the admin
//! surface and the partner console. **Folded into `oxy seed`** (no `--partners`
//! flag); silently skipped on a non-local DB.
//!
//! The seed IS the design's demo (see
//! `internal-docs/partner-platform.md`). It proves two things
//! the old seed structurally could not:
//!
//! 1. **A partner is a real org.** Acme Consulting has its own owner, its own
//!    workspace, and — with 6 members but only 5 operator slots — an employee who
//!    holds **no partner role at all**, just using Acme's own Oxy. The partner is
//!    not a management shell.
//!
//! 2. **Access is one thing, bounded by the ceiling.** Acme's operators each reach
//!    every client Acme manages and can do everything Acme's ceiling allows — no
//!    per-person roles, no per-client scopes. Initech shows the ceiling rule: its
//!    operators get only `manage_apps` + `view_audit`, so that is all any of them
//!    can do — not manage members, query data, or onboard clients.
//!
//! People (name + email) are GENERATED from curated pools, deterministically, so
//! the data is convincing AND reproducible. Deterministic UUID v5 ids + check-
//! first inserts make the whole seed idempotent and test-referenceable.
//!
//! Dev/test only — refuses a non-local `OXY_DATABASE_URL` for the destructive
//! `--clear`, and skips the partner seed for a non-local `oxy seed`.

use chrono::Utc;
use entity::org_members::{self, OrgRole};
use entity::prelude::{
    OrgMembers, Organizations, PartnerCapabilities, PartnerGrants, PartnerOrgs,
    PartnerRoleBindings, Workspaces,
};
use entity::workspaces::{self, WorkspaceStatus};
use entity::{
    organizations, partner_capabilities, partner_grants, partner_orgs, partner_role_bindings,
};
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_auth::types::Identity;
use oxy_auth::user::UserService;
use oxy_shared::errors::OxyError;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use uuid::Uuid;

type Conn = sea_orm::DatabaseConnection;

/// One seeded org — its identity, how many people to generate, and its
/// workspaces. People (realistic name + matching email) are GENERATED, so this
/// list stays about topology, not hand-typed `owner`/`analyst` strings.
struct OrgSeed {
    slug: &'static str,
    name: &'static str,
    /// Members to generate besides the owner (person 0).
    members: usize,
    /// Workspace display names; each is pointed at the demo project.
    workspaces: &'static [&'static str],
}

/// A partnership: the org that holds it, its clients, its **ceiling**, and how
/// many of its people are **operators**. `ceiling` = [members, apps, develop_apps,
/// audit, billing, secrets, create_orgs, org_settings].
struct PartnerSeed {
    org_slug: &'static str,
    manages: &'static [&'static str],
    ceiling: [bool; 8],
    /// How many of the partner org's people (owner first, then members) get
    /// partner access. Fewer than the org has leaves an ordinary employee with
    /// NO access — the "sees no console" case.
    operators: usize,
}

/// A generated person — a realistic name and a matching email.
struct Person {
    name: String,
    email: String,
}

// Curated realistic name pools. Index-paired (`first[i]`/`last[i]`) so the i-th
// person is a stable, unique "First Last" — far more convincing than the old
// role-strings, and fully reproducible (the seed keys UUIDs off the email).
const FIRST_NAMES: &[&str] = &[
    "Maya", "Oliver", "Priya", "Diego", "Sofia", "Liam", "Amara", "Noah", "Yuki", "Elena", "Omar",
    "Grace", "Kenji", "Fatima", "Lucas", "Nadia", "Theo", "Ingrid", "Marcus", "Leila", "Ravi",
    "Chloe", "Andre", "Zara", "Hugo", "Mira", "Silas", "Nora", "Idris", "Vera",
];
const LAST_NAMES: &[&str] = &[
    "Nguyen",
    "Okafor",
    "Silva",
    "Kim",
    "Rossi",
    "Haddad",
    "Larsson",
    "Mehta",
    "Costa",
    "Dubois",
    "Tanaka",
    "Flores",
    "Novak",
    "Reyes",
    "Adeyemi",
    "Cohen",
    "Bauer",
    "Petrov",
    "Santos",
    "Ivanova",
    "Khan",
    "Werner",
    "Fontaine",
    "Diallo",
    "Marino",
    "Bishop",
    "Sato",
    "Lindqvist",
    "Osei",
    "Vargas",
];

/// The i-th person at `org_slug`: a realistic name + a matching email
/// (`first.last@<slug>.test`). Deterministic, and — for `i` within the pool —
/// unique within the org (index differs) and across orgs (domain differs).
fn person(i: usize, org_slug: &str) -> Person {
    let first = FIRST_NAMES[i % FIRST_NAMES.len()];
    let last = LAST_NAMES[i % LAST_NAMES.len()];
    Person {
        name: format!("{first} {last}"),
        email: format!(
            "{}.{}@{org_slug}.test",
            first.to_lowercase(),
            last.to_lowercase()
        ),
    }
}

const ORGS: &[OrgSeed] = &[
    // The partner — a REAL org. Its people work here; it has a workspace of its
    // own (the point of collapsing partners into orgs). Owner + 6 members = 7
    // people, so a partnership can leave one without operator access.
    OrgSeed {
        slug: "acme",
        name: "Acme Consulting",
        members: 6,
        workspaces: &["Acme Internal Analytics"],
    },
    OrgSeed {
        slug: "northwind",
        name: "Northwind Traders",
        members: 2,
        workspaces: &["Northwind Analytics", "Northwind Sandbox"],
    },
    OrgSeed {
        slug: "globex",
        name: "Globex Corporation",
        members: 2,
        workspaces: &["Globex Analytics", "Globex Sandbox"],
    },
    // The second partner — narrow ceiling (see PARTNERS).
    OrgSeed {
        slug: "initech",
        name: "Initech",
        members: 2,
        workspaces: &["Initech Analytics"],
    },
    OrgSeed {
        slug: "umbrella",
        name: "Umbrella Industries",
        members: 1,
        workspaces: &["Umbrella Analytics"],
    },
    // Deliberately UNMANAGED — no partner. The admin UI should show both states.
    OrgSeed {
        slug: "vandelay",
        name: "Vandelay Industries",
        members: 1,
        workspaces: &["Vandelay Analytics"],
    },
];

const PARTNERS: &[PartnerSeed] = &[
    PartnerSeed {
        org_slug: "acme",
        manages: &["northwind", "globex"],
        // A broad ceiling: Acme may do everything EXCEPT billing and secrets —
        // the two flags that stay Owner-only to grant.
        //   members, apps, develop, audit, billing, secrets, create_orgs, settings
        ceiling: [true, true, true, true, false, false, true, true],
        // Owner + 5 of 6 members are operators; person 6 (the last member) is an
        // ordinary Acme employee with NO partner access — must see no console.
        operators: 6,
    },
    PartnerSeed {
        org_slug: "initech",
        manages: &["umbrella"],
        // A NARROW ceiling: apps + audit only. Operators can publish apps and read
        // Umbrella's audit log — and nothing else (not manage members, query data,
        // or onboard clients): what Oxy grants the partner is the whole story, the
        // same for every operator.
        ceiling: [false, true, false, true, false, false, false, false],
        // Owner + both members.
        operators: 3,
    },
];

/// Deterministic UUID v5 for a seeded row — stable across machines/re-runs, so
/// tests can hard-code ids. Namespaced by row `kind` to avoid collisions.
fn seed_id(kind: &str, key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("oxy.partner-seed.{kind}.{key}").as_bytes(),
    )
}

/// Refuse to run against a database that looks like production.
///
/// The seed creates real users through the real sign-in path and grants a real
/// partnership over real orgs. "Dev/test only" in a docstring stops nobody with a
/// prod `OXY_DATABASE_URL` exported in their shell — which is a normal state for an
/// on-call engineer. The `.test` emails make it survivable, not safe.
///
/// Opt out with `OXY_SEED_ALLOW_REMOTE=1` when you genuinely mean it (a shared
/// staging box), so the guard is an obstacle to accidents, not to work.
/// Does the target DB look local (or was remote seeding explicitly allowed)? The
/// seed creates real users, orgs and a partner grant, so it must not touch prod.
fn is_local() -> bool {
    if std::env::var("OXY_SEED_ALLOW_REMOTE").is_ok() {
        return true;
    }
    let url = std::env::var("OXY_DATABASE_URL").unwrap_or_default();
    url.is_empty()
        || url.contains("localhost")
        || url.contains("127.0.0.1")
        || url.contains("@postgres")
        || url.contains("host.docker.internal")
}

/// Hard error when the DB isn't local — used by the destructive `--clear` path,
/// where silently skipping would be more surprising than refusing. `pub(crate)`
/// so the seed command can guard the WHOLE clear branch up front (clear_demo has
/// no gate of its own).
pub(crate) fn refuse_if_not_local() -> Result<(), OxyError> {
    if is_local() {
        return Ok(());
    }
    let url = std::env::var("OXY_DATABASE_URL").unwrap_or_default();
    Err(OxyError::ConfigurationError(format!(
        "refusing to seed: OXY_DATABASE_URL does not look local ({}). \
         This creates users, orgs and a partner grant. Set OXY_SEED_ALLOW_REMOTE=1 if you mean it.",
        url.split('@').next_back().unwrap_or("<hidden>")
    )))
}

/// Seed all tenant + partner test data. Idempotent. `demo_path` is the demo
/// project every seeded workspace points at. On a non-local DB this **skips**
/// (does not error) so a folded `oxy seed` still seeds the demo workspace safely.
pub async fn seed_partner_tenants(demo_path: &str) -> Result<(), OxyError> {
    if !is_local() {
        println!(
            "{} skipping partner/tenant seed — OXY_DATABASE_URL is not local \
             (set OXY_SEED_ALLOW_REMOTE=1 to force)",
            "⚠️".info()
        );
        return Ok(());
    }
    println!("{} seeding partner + tenant test data", "🌱".info());
    let conn = establish_connection().await?;

    for org in ORGS {
        seed_org(&conn, org, demo_path).await?;
    }
    for partner in PARTNERS {
        seed_partner(&conn, partner).await?;
    }

    print_summary();
    Ok(())
}

/// Ensure one org, its owner + generated members, and its workspaces exist.
async fn seed_org(conn: &Conn, org: &OrgSeed, demo_path: &str) -> Result<(), OxyError> {
    let org_id = ensure_org(conn, org.slug, org.name).await?;

    // Person 0 is the owner; 1..=members are members — realistic + deterministic.
    for i in 0..=org.members {
        let p = person(i, org.slug);
        let uid = ensure_user(&p.email, &p.name).await?;
        let role = if i == 0 {
            OrgRole::Owner
        } else {
            OrgRole::Member
        };
        ensure_org_member(conn, org_id, uid, role).await?;
    }
    for name in org.workspaces {
        ensure_workspace(conn, org_id, name, demo_path).await?;
    }
    println!(
        "  {} org {} ({}) — {} people",
        "✓".success(),
        org.slug,
        org.name,
        org.members + 1
    );
    Ok(())
}

/// Ensure one partnership: the grant, its ceiling, its clients, and its people's
/// role bindings.
async fn seed_partner(conn: &Conn, partner: &PartnerSeed) -> Result<(), OxyError> {
    let Some(partner_org_id) = find_org_id(conn, partner.org_slug).await? else {
        return Ok(());
    };
    ensure_grant(conn, partner_org_id).await?;
    ensure_ceiling(conn, partner_org_id, partner.ceiling).await?;

    for slug in partner.manages {
        if let Some(org_id) = find_org_id(conn, slug).await? {
            ensure_partner_org(conn, partner_org_id, org_id).await?;
        }
    }

    for i in 0..partner.operators {
        // A partner's operators are ORDINARY MEMBERS of the partner org (person i,
        // owner first). The access row hangs off that membership.
        let p = person(i, partner.org_slug);
        let user_id = ensure_user(&p.email, &p.name).await?;
        let Some(member) = OrgMembers::find()
            .filter(org_members::Column::OrgId.eq(partner_org_id))
            .filter(org_members::Column::UserId.eq(user_id))
            .one(conn)
            .await
            .map_err(|e| OxyError::DBError(format!("query member {}: {e}", p.email)))?
        else {
            continue;
        };

        ensure_binding(conn, member.id).await?;
    }

    println!(
        "  {} partner {} → {} client(s), {} operator(s)",
        "✓".success(),
        partner.org_slug,
        partner.manages.len(),
        partner.operators
    );
    Ok(())
}

async fn ensure_org(conn: &Conn, slug: &str, name: &str) -> Result<Uuid, OxyError> {
    if let Some(existing) = Organizations::find()
        .filter(organizations::Column::Slug.eq(slug))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query org {slug}: {e}")))?
    {
        return Ok(existing.id);
    }
    let now = Utc::now().fixed_offset();
    let id = seed_id("org", slug);
    organizations::ActiveModel {
        id: ActiveValue::Set(id),
        name: ActiveValue::Set(name.to_string()),
        slug: ActiveValue::Set(slug.to_string()),
        logo: ActiveValue::NotSet,
        logo_content_type: ActiveValue::NotSet,
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert org {slug}: {e}")))?;
    Ok(id)
}

async fn find_org_id(conn: &Conn, slug: &str) -> Result<Option<Uuid>, OxyError> {
    Ok(Organizations::find()
        .filter(organizations::Column::Slug.eq(slug))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("lookup org {slug}: {e}")))?
        .map(|o| o.id))
}

/// Email-keyed, idempotent user create via `UserService` (same path as OAuth
/// login), so a seeded email can sign in with a magic link.
async fn ensure_user(email: &str, name: &str) -> Result<Uuid, OxyError> {
    let user = UserService::get_or_create_user(&Identity {
        email: email.to_string(),
        name: Some(name.to_string()),
        picture: None,
    })
    .await?;
    Ok(user.id)
}

async fn ensure_org_member(
    conn: &Conn,
    org_id: Uuid,
    user_id: Uuid,
    role: OrgRole,
) -> Result<(), OxyError> {
    let existing = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query org_member: {e}")))?;
    if existing.is_some() {
        return Ok(());
    }
    let now = Utc::now().fixed_offset();
    org_members::ActiveModel {
        id: ActiveValue::Set(seed_id("org_member", &format!("{org_id}:{user_id}"))),
        org_id: ActiveValue::Set(org_id),
        user_id: ActiveValue::Set(user_id),
        role: ActiveValue::Set(role),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert org_member: {e}")))?;
    Ok(())
}

/// A workspace row pointed at the demo project (`demo_path`), so a seeded org's
/// workspace opens to real content instead of an empty shell.
async fn ensure_workspace(
    conn: &Conn,
    org_id: Uuid,
    name: &str,
    demo_path: &str,
) -> Result<(), OxyError> {
    let id = seed_id("workspace", &format!("{org_id}:{name}"));
    if let Some(row) = Workspaces::find_by_id(id)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query workspace {name}: {e}")))?
    {
        // Patch the path if it's missing or stale. Earlier seeds created these
        // path-less, which made the recovery/latency worker skip + warn on them
        // ("workspace has no path"); a re-run of `oxy seed` now heals them.
        if row.path.as_deref() != Some(demo_path) {
            let mut active = row.into_active_model();
            active.path = ActiveValue::Set(Some(demo_path.to_string()));
            active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
            active
                .update(conn)
                .await
                .map_err(|e| OxyError::DBError(format!("patch workspace {name} path: {e}")))?;
        }
        return Ok(());
    }
    let now = Utc::now().fixed_offset();
    workspaces::ActiveModel {
        id: ActiveValue::Set(id),
        name: ActiveValue::Set(name.to_string()),
        git_namespace_id: ActiveValue::Set(None),
        git_remote_url: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        path: ActiveValue::Set(Some(demo_path.to_string())),
        last_opened_at: ActiveValue::Set(None),
        created_by: ActiveValue::Set(None),
        org_id: ActiveValue::Set(Some(org_id)),
        status: ActiveValue::Set(WorkspaceStatus::Ready),
        error: ActiveValue::Set(None),
        monthly_vlm_budget_micros: ActiveValue::Set(None),
        current_revision_id: ActiveValue::Set(None),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert workspace {name}: {e}")))?;
    Ok(())
}

/// The partnership itself — keyed on the org, because the partner IS the org.
async fn ensure_grant(conn: &Conn, org_id: Uuid) -> Result<(), OxyError> {
    if PartnerGrants::find_by_id(org_id)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query partner_grant: {e}")))?
        .is_some()
    {
        return Ok(());
    }
    partner_grants::ActiveModel {
        org_id: ActiveValue::Set(org_id),
        status: ActiveValue::Set("active".to_string()),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert partner_grant: {e}")))?;
    Ok(())
}

/// The ceiling — what Oxy permits this partner AT ALL. Every role a partner hands
/// out is intersected with this.
async fn ensure_ceiling(conn: &Conn, org_id: Uuid, caps: [bool; 8]) -> Result<(), OxyError> {
    if PartnerCapabilities::find_by_id(org_id)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query partner_capabilities: {e}")))?
        .is_some()
    {
        return Ok(());
    }
    partner_capabilities::ActiveModel {
        org_id: ActiveValue::Set(org_id),
        manage_members: ActiveValue::Set(caps[0]),
        manage_apps: ActiveValue::Set(caps[1]),
        develop_apps: ActiveValue::Set(caps[2]),
        view_audit: ActiveValue::Set(caps[3]),
        manage_billing: ActiveValue::Set(caps[4]),
        manage_secrets: ActiveValue::Set(caps[5]),
        create_orgs: ActiveValue::Set(caps[6]),
        manage_org_settings: ActiveValue::Set(caps[7]),
        updated_at: ActiveValue::Set(Utc::now().fixed_offset()),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert partner_capabilities: {e}")))?;
    Ok(())
}

/// Attach a client. `managed_org_id` is UNIQUE — one partner per client — so if
/// the org is already attached we leave the existing link alone.
async fn ensure_partner_org(
    conn: &Conn,
    partner_org_id: Uuid,
    managed_org_id: Uuid,
) -> Result<(), OxyError> {
    if PartnerOrgs::find()
        .filter(partner_orgs::Column::ManagedOrgId.eq(managed_org_id))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query partner_org: {e}")))?
        .is_some()
    {
        return Ok(());
    }
    partner_orgs::ActiveModel {
        id: ActiveValue::Set(seed_id("partner_org", &managed_org_id.to_string())),
        partner_org_id: ActiveValue::Set(partner_org_id),
        managed_org_id: ActiveValue::Set(managed_org_id),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert partner_org: {e}")))?;
    Ok(())
}

/// Partner access for this member — one row means they're an operator.
async fn ensure_binding(conn: &Conn, org_member_id: Uuid) -> Result<Uuid, OxyError> {
    if let Some(existing) = PartnerRoleBindings::find()
        .filter(partner_role_bindings::Column::OrgMemberId.eq(org_member_id))
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query partner_role_binding: {e}")))?
    {
        return Ok(existing.id);
    }
    let id = seed_id("partner_binding", &org_member_id.to_string());
    partner_role_bindings::ActiveModel {
        id: ActiveValue::Set(id),
        org_member_id: ActiveValue::Set(org_member_id),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert partner_role_binding: {e}")))?;
    Ok(id)
}

fn print_summary() {
    let users: usize = ORGS.iter().map(|o| o.members + 1).sum();
    let workspaces: usize = ORGS.iter().map(|o| o.workspaces.len()).sum();
    println!(
        "{} seeded {} orgs, {} users, {} workspaces, {} partnership(s)",
        "✅".success(),
        ORGS.len(),
        users,
        workspaces,
        PARTNERS.len()
    );
    println!("\n  Sign in with a magic link as any owner below (or any first.last@<org>.test):");
    for o in ORGS {
        let owner = person(0, o.slug);
        let role = if let Some(p) = PARTNERS.iter().find(|p| p.org_slug == o.slug) {
            format!(
                "PARTNER — {} operators, manages {}",
                p.operators,
                p.manages.join(", ")
            )
        } else if PARTNERS.iter().any(|p| p.manages.contains(&o.slug)) {
            "managed client".to_string()
        } else {
            "unmanaged".to_string()
        };
        println!(
            "    {:<20} owner {} <{}> — {role}",
            o.name, owner.name, owner.email
        );
    }
}

/// Tear down seeded partner + tenant rows. Deleting the orgs cascades to the
/// grants, ceilings, client links, role bindings and assignments (every FK is
/// `on_delete = Cascade`). Seeded users are left in place, like `clear_demo`.
pub async fn clear_partner_tenants() -> Result<(), OxyError> {
    // Deleting orgs cascades to members, workspaces and partnerships — the last
    // thing you want pointed at prod by accident.
    refuse_if_not_local()?;
    let conn = establish_connection().await?;
    let mut removed = 0u64;
    for org in ORGS {
        removed += Organizations::delete_many()
            .filter(organizations::Column::Slug.eq(org.slug))
            .exec(&conn)
            .await
            .map_err(|e| OxyError::DBError(format!("delete org {}: {e}", org.slug)))?
            .rows_affected;
    }
    println!(
        "{} cleared {removed} seeded org(s) (cascades removed partnerships, members + workspaces)",
        "🧹".info()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn person_is_deterministic_and_realistic() {
        // Same (index, org) → same person, so the seed is reproducible.
        assert_eq!(person(0, "acme").email, person(0, "acme").email);
        let p = person(0, "acme");
        // A real "First Last" and a matching first.last@org.test — not "owner".
        assert!(
            p.name.contains(' '),
            "name should be 'First Last': {}",
            p.name
        );
        assert_ne!(p.name.to_lowercase(), "owner");
        assert!(p.email.ends_with("@acme.test"), "email: {}", p.email);
        assert!(
            p.email.contains('.'),
            "email should be first.last: {}",
            p.email
        );
    }

    #[test]
    fn people_are_unique_within_an_org() {
        // acme: owner + 6 members = person(0..=6). All seven emails distinct, so
        // no two seeded users collide.
        let emails: HashSet<_> = (0..=6).map(|i| person(i, "acme").email).collect();
        assert_eq!(emails.len(), 7);
    }

    #[test]
    fn pools_cover_the_biggest_org() {
        // person() indexes the pools mod their length; keep the largest org within
        // the pool so its members stay unique.
        let max_people = ORGS.iter().map(|o| o.members + 1).max().unwrap();
        assert!(max_people <= FIRST_NAMES.len(), "grow FIRST_NAMES");
        assert!(max_people <= LAST_NAMES.len(), "grow LAST_NAMES");
    }

    #[test]
    fn partners_reference_real_orgs_and_fit_their_people() {
        for p in PARTNERS {
            let org = ORGS
                .iter()
                .find(|o| o.slug == p.org_slug)
                .unwrap_or_else(|| panic!("partner org {} not in ORGS", p.org_slug));
            // Can't grant access to more people than the org has.
            assert!(
                p.operators <= org.members + 1,
                "{}: {} operators but {} people",
                p.org_slug,
                p.operators,
                org.members + 1
            );
            // Every managed client is a real seeded org too.
            for client in p.manages {
                assert!(
                    ORGS.iter().any(|o| o.slug == *client),
                    "{} manages unknown org {client}",
                    p.org_slug
                );
            }
        }
    }

    #[test]
    fn acme_leaves_one_employee_without_partner_access() {
        // The deliberate test case: an ordinary employee who must see no console.
        let acme = ORGS.iter().find(|o| o.slug == "acme").unwrap();
        let partner = PARTNERS.iter().find(|p| p.org_slug == "acme").unwrap();
        assert!(
            partner.operators < acme.members + 1,
            "acme should leave >=1 member without partner access"
        );
    }
}
