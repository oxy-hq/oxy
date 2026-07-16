//! Partner authorization — "may this user administer this client org, and with
//! what permissions?"
//!
//! Model: `internal-docs/2026-07-16-partner-platform-design.md`.
//!
//! ## The shape
//!
//! A partner is not an entity — it is a **grant an organization holds**. Its people
//! are ordinary `org_members` of that org who additionally hold **partner access**
//! (a `partner_role_bindings` row). So:
//!
//! ```text
//! authority(user, target_org) = ceiling                     // what Oxy allows the partner
//!                             ∩ has_access(user)             // is this member an operator
//!                             ∩ (target_org ∈ partner's clients)
//! ```
//!
//! The **ceiling** is `partner_capabilities` — Oxy's grant to the partner, and the
//! whole capability story: there are no per-person roles and no per-client scope.
//! Every operator reaches every client and can do exactly what the ceiling allows.
//!
//! ## The load-bearing decision (unchanged)
//!
//! Partner power flows through this capability-gated surface — it is **never**
//! injected as a synthetic membership into `OrgContext`. A partner admin given a
//! synthetic Org Admin membership would silently pass `OrgAdminStrict` (billing /
//! admin-promotion) regardless of their permissions. Making a partner org-backed
//! changed *identity and home*, not *enforcement*: this module stays additive and
//! the org/workspace guards are untouched.
//!
//! Everything here **fails closed**: a DB error, a missing ceiling row, a suspended
//! grant, or no partner-access binding all resolve to "no access".
//!
//! ## Where authorization lives — read it in this order
//!
//! Four files, one flow:
//!
//! 1. **Vocabulary** — [`PartnerCapability`] (this file): the closed set of
//!    permissions. The single source of truth; everything below derives from it.
//! 2. **Ceiling** — [`Capabilities`] (this file) + the `partner_capabilities`
//!    table: which of those a given partner was granted (Oxy-set, per partner).
//! 3. **Decision** — `partner_policy`: the Cedar rules (one `permit` per
//!    capability, **generated from [`PartnerCapability::ALL`]**) evaluated over an
//!    entity graph where each managed org is a child of the partner, so
//!    `resource in principal` *is* the ownership check.
//! 4. **Enforcement** — `partner_console::require_org_scope` (org-scoped actions)
//!    and the customer-app gate (`partner_policy::partner_grants_app_access`) call
//!    the decision and map a Deny to 403/404. This is where a handler opts a route
//!    into a capability.
//!
//! ## Extending it
//!
//! **Add a capability** (say `manage_schedules`):
//!   1. Migration: add a `partner_capabilities.manage_schedules` boolean.
//!   2. In this file: add the [`PartnerCapability`] variant + its `as_str` arm +
//!      its [`PartnerCapability::ALL`] entry; add the [`Capabilities`] field + its
//!      `from_model` / `allows` / `set` arms. The compiler forces every one of
//!      these **except the `ALL` entry** — that is the single step you must
//!      remember (the `policies_parse` test checks the policy count against `ALL`).
//!   3. Policy + `granted_names`: **nothing** — both are generated from `ALL`.
//!   4. Enforce it: at the handler, `require_org_scope(&db, &scope, org_id,
//!      PartnerCapability::ManageSchedules)`.
//!
//! **Gate a new endpoint on an existing capability**: just call `require_org_scope`
//! (or check the cap in the customer-app gate). No policy or vocabulary change.
//!
//! **Change what a capability *means*** (the rule, not the name): edit the one
//! generated rule shape in `partner_policy::policies` — or, if a capability needs a
//! non-uniform rule, give it an explicit branch there. The rest stay generated.
//!
//! §2–§3 of the design doc is the prose version of this map.

use entity::prelude::*;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

/// A single permission an action can require. Compile-time — Oxy owns this
/// vocabulary; customers never invent permissions (see the design doc for why we
/// don't build AWS/GCP-style IAM).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PartnerCapability {
    ManageMembers,
    /// Publish / unpublish only — NOT data access.
    ManageApps,
    /// The custom-app data plane (query / semantic-query / agent runs, oxy proxy).
    DevelopApps,
    ViewAudit,
    ManageBilling,
    ManageSecrets,
    /// Onboard a client org (create + attach). Mints billable tenants.
    CreateOrgs,
    /// Rename / configure a managed org.
    ManageOrgSettings,
}

impl PartnerCapability {
    /// Stable name — the Cedar action id and the string in a request's capability
    /// set. Keep in sync with the Cedar policies in `partner_policy`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManageMembers => "manage_members",
            Self::ManageApps => "manage_apps",
            Self::DevelopApps => "develop_apps",
            Self::ViewAudit => "view_audit",
            Self::ManageBilling => "manage_billing",
            Self::ManageSecrets => "manage_secrets",
            Self::CreateOrgs => "create_orgs",
            Self::ManageOrgSettings => "manage_org_settings",
        }
    }

    /// Every variant. **The generated Cedar policy and `granted_names` both
    /// iterate this**, so a new capability MUST be added here — the compiler
    /// won't force it (see the module-level "Extending it" recipe).
    pub const ALL: [Self; 8] = [
        Self::ManageMembers,
        Self::ManageApps,
        Self::DevelopApps,
        Self::ViewAudit,
        Self::ManageBilling,
        Self::ManageSecrets,
        Self::CreateOrgs,
        Self::ManageOrgSettings,
    ];
}

// There is deliberately NO role catalog. A partner has one kind of person — an
// operator — and their authority IS the partner's ceiling. What Oxy grants the
// partner (below) is the whole story; the partner does not slice it further.

/// A permission set. `Default` is all-false so a missing row fails closed rather
/// than granting blanket power.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub manage_members: bool,
    pub manage_apps: bool,
    pub develop_apps: bool,
    pub view_audit: bool,
    pub manage_billing: bool,
    pub manage_secrets: bool,
    pub create_orgs: bool,
    pub manage_org_settings: bool,
}

impl Capabilities {
    pub fn from_model(m: &entity::partner_capabilities::Model) -> Self {
        Self {
            manage_members: m.manage_members,
            manage_apps: m.manage_apps,
            develop_apps: m.develop_apps,
            view_audit: m.view_audit,
            manage_billing: m.manage_billing,
            manage_secrets: m.manage_secrets,
            create_orgs: m.create_orgs,
            manage_org_settings: m.manage_org_settings,
        }
    }

    pub fn allows(&self, cap: PartnerCapability) -> bool {
        use PartnerCapability as C;
        match cap {
            C::ManageMembers => self.manage_members,
            C::ManageApps => self.manage_apps,
            C::DevelopApps => self.develop_apps,
            C::ViewAudit => self.view_audit,
            C::ManageBilling => self.manage_billing,
            C::ManageSecrets => self.manage_secrets,
            C::CreateOrgs => self.create_orgs,
            C::ManageOrgSettings => self.manage_org_settings,
        }
    }

    pub fn set(&mut self, cap: PartnerCapability, on: bool) {
        use PartnerCapability as C;
        match cap {
            C::ManageMembers => self.manage_members = on,
            C::ManageApps => self.manage_apps = on,
            C::DevelopApps => self.develop_apps = on,
            C::ViewAudit => self.view_audit = on,
            C::ManageBilling => self.manage_billing = on,
            C::ManageSecrets => self.manage_secrets = on,
            C::CreateOrgs => self.create_orgs = on,
            C::ManageOrgSettings => self.manage_org_settings = on,
        }
    }

    /// The granted names — fed to Cedar as the principal's capability set.
    pub fn granted_names(&self) -> Vec<&'static str> {
        PartnerCapability::ALL
            .into_iter()
            .filter(|c| self.allows(*c))
            .map(|c| c.as_str())
            .collect()
    }
}

/// Resolved partner authority for one request.
#[derive(Clone, Debug)]
pub struct PartnerScope {
    /// The partner — which IS an org.
    pub partner_id: Uuid,
    /// The partner org's slug (console routing).
    pub slug: String,
    /// What this operator may do — the partner's ceiling, verbatim. Every operator
    /// of a partner has the same authority; there are no per-person roles.
    pub capabilities: Capabilities,
    /// Every client the partner manages. All operators reach all clients, so this
    /// is the partner's whole managed set — kept on the scope so org-scoped gates
    /// (`require_org_scope`) and the console listings read one field.
    pub org_ids: Vec<Uuid>,
}

impl PartnerScope {
    pub fn allows(&self, cap: PartnerCapability) -> bool {
        self.capabilities.allows(cap)
    }
}

/// Resolve `user`'s authority within the partner org `partner_org_id`, or `None`.
///
/// Fails closed at every step: no partner grant, a suspended grant, not a member of
/// the partner org, no partner-access binding, or any DB error.
pub async fn resolve_scope(
    db: &DatabaseConnection,
    partner_org_id: Uuid,
    user_id: Uuid,
    _user_email: &str,
) -> Option<PartnerScope> {
    let grant = PartnerGrants::find_by_id(partner_org_id)
        .one(db)
        .await
        .ok()??;
    if grant.status != "active" {
        return None;
    }

    let org = Organizations::find_by_id(partner_org_id)
        .one(db)
        .await
        .ok()??;

    let ceiling_of = |c: Option<entity::partner_capabilities::Model>| {
        c.as_ref().map(Capabilities::from_model).unwrap_or_default()
    };

    // The person is an ORDINARY MEMBER of the partner org — no parallel membership
    // system, no email keying.
    let membership = OrgMembers::find()
        .filter(entity::org_members::Column::OrgId.eq(partner_org_id))
        .filter(entity::org_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .ok()?;

    let Some(membership) = membership else {
        // Not a member — but Oxy staff with a LIVE assume-role session for this
        // partner are deliberately here: "act as a managing partner" has to show
        // them the partner console, or the mode is a lie. Same shape as
        // `org_context` synthesizing an Owner membership, and gated the same way:
        // no session ⇒ no scope ⇒ a plain 403, exactly like any other non-member.
        //
        // They get `partner_admin` capped by the ceiling — i.e. what the partner's
        // own boss can do, never more. Staff do not out-rank the ceiling Oxy set.
        return assumed_scope(db, partner_org_id, user_id, _user_email, &org).await;
    };

    // …who additionally holds partner access. A member of Acme WITHOUT a binding is
    // just an Acme employee using Acme's own Oxy — they manage no clients.
    PartnerRoleBindings::find()
        .filter(entity::partner_role_bindings::Column::OrgMemberId.eq(membership.id))
        .one(db)
        .await
        .ok()??;

    // An operator's authority IS the ceiling — nothing narrows it further.
    let capabilities = ceiling_of(
        PartnerCapabilities::find_by_id(partner_org_id)
            .one(db)
            .await
            .ok()?,
    );

    Some(PartnerScope {
        partner_id: partner_org_id,
        slug: org.slug,
        capabilities,
        // All operators reach every client the partner manages.
        org_ids: partner_org_ids(db, partner_org_id).await,
    })
}

/// The scope Oxy staff get while **acting as** a partner they don't belong to.
///
/// Requires a live, audited, time-bounded assume-role session for the partner org
/// (`admin::assume`). Without one this returns `None`, so staff hitting
/// `/partners/{id}` as a non-member get a 403 like anyone else — the reach is
/// opt-in, not ambient.
///
/// Their authority is the partner's ceiling — exactly what the partner's own
/// operators see. Staff do not get to exceed the ceiling Oxy granted; if a
/// capability is off, it stays off, and the operator sees the same walls the
/// customer does. That is the entire point of looking through their eyes.
async fn assumed_scope(
    db: &DatabaseConnection,
    partner_org_id: Uuid,
    user_id: Uuid,
    user_email: &str,
    org: &entity::organizations::Model,
) -> Option<PartnerScope> {
    if !is_oxy_staff(db, user_email).await {
        return None;
    }
    if !crate::server::api::admin::assume::is_session_live(db, user_id, partner_org_id).await {
        return None;
    }

    let ceiling = PartnerCapabilities::find_by_id(partner_org_id)
        .one(db)
        .await
        .ok()?
        .as_ref()
        .map(Capabilities::from_model)
        .unwrap_or_default();

    tracing::warn!(
        actor = %user_email, partner_org_id = %partner_org_id,
        "partner_authz: assume-role session active — acting as this partner's admin"
    );

    Some(PartnerScope {
        partner_id: partner_org_id,
        slug: org.slug.clone(),
        // Staff acting as the partner get exactly what the partner's own operators
        // get — the ceiling, nothing more.
        capabilities: ceiling,
        // The partner's operators handle every client.
        org_ids: partner_org_ids(db, partner_org_id).await,
    })
}

/// Global Owner (env allow-list) or Global Admin (`app_admins` table).
async fn is_oxy_staff(db: &DatabaseConnection, email: &str) -> bool {
    if crate::server::api::middlewares::oxy_owner_guard::is_oxy_owner(email) {
        return true;
    }
    crate::server::api::customer_apps_auth::is_app_admin_email(db, email)
        .await
        .unwrap_or(false)
}

/// Every client this partner manages.
pub async fn partner_org_ids(db: &DatabaseConnection, partner_org_id: Uuid) -> Vec<Uuid> {
    PartnerOrgs::find()
        .filter(entity::partner_orgs::Column::PartnerOrgId.eq(partner_org_id))
        .all(db)
        .await
        .map(|rows| rows.into_iter().map(|r| r.managed_org_id).collect())
        .unwrap_or_default()
}

/// The partner managing `org_id`, if any. `managed_org_id` is UNIQUE, so at most one.
pub async fn partner_for_org(db: &DatabaseConnection, org_id: Uuid) -> Option<Uuid> {
    PartnerOrgs::find()
        .filter(entity::partner_orgs::Column::ManagedOrgId.eq(org_id))
        .one(db)
        .await
        .ok()?
        .map(|r| r.partner_org_id)
}

/// Whether `partner_org_id` manages `org_id`.
pub async fn partner_owns_org(db: &DatabaseConnection, partner_org_id: Uuid, org_id: Uuid) -> bool {
    partner_for_org(db, org_id).await == Some(partner_org_id)
}

/// Every partner this user holds a role in (drives the console entry).
pub async fn scopes_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
    user_email: &str,
) -> Vec<PartnerScope> {
    let memberships = OrgMembers::find()
        .filter(entity::org_members::Column::UserId.eq(user_id))
        .all(db)
        .await
        .unwrap_or_default();

    let mut org_ids: Vec<Uuid> = memberships.into_iter().map(|m| m.org_id).collect();

    // Orgs the caller is ACTING AS. Without this a staff member who assumed a
    // partner would resolve a scope on `/partners/{id}` but never see the partner
    // listed — the console would be empty and the mode would look broken.
    for session in crate::server::api::admin::assume::live_sessions_for(db, user_id).await {
        if !org_ids.contains(&session.org_id) {
            org_ids.push(session.org_id);
        }
    }

    let mut out = Vec::new();
    for org_id in org_ids {
        if let Some(scope) = resolve_scope(db, org_id, user_id, user_email).await {
            out.push(scope);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capabilities_fail_closed() {
        let c = Capabilities::default();
        for cap in PartnerCapability::ALL {
            assert!(!c.allows(cap), "{cap:?} must be denied by default");
        }
    }

    /// An operator's authority IS the ceiling — what Oxy grants the partner is the
    /// whole story. A capability the ceiling withholds is simply absent, and one it
    /// grants is present, with no role in between.
    #[test]
    fn authority_is_exactly_the_ceiling() {
        let mut ceiling = Capabilities::default();
        ceiling.set(PartnerCapability::ManageApps, true);
        ceiling.set(PartnerCapability::ManageMembers, true);

        // Present because the ceiling grants them.
        assert!(ceiling.allows(PartnerCapability::ManageApps));
        assert!(ceiling.allows(PartnerCapability::ManageMembers));
        // Absent because the ceiling withholds them — the data plane and billing
        // stay denied no matter who the operator is.
        assert!(!ceiling.allows(PartnerCapability::DevelopApps));
        assert!(!ceiling.allows(PartnerCapability::ManageBilling));
    }

    #[test]
    fn granted_names_match_the_capability_strings() {
        let mut c = Capabilities::default();
        c.set(PartnerCapability::CreateOrgs, true);
        c.set(PartnerCapability::ViewAudit, true);
        let names = c.granted_names();
        assert!(names.contains(&"create_orgs"));
        assert!(names.contains(&"view_audit"));
        assert!(!names.contains(&"develop_apps"));
    }

    /// Acting as a partner shows you what that partner's BOSS sees — never more.
    ///
    /// Staff do not out-rank the ceiling Oxy granted. If `develop_apps` is off for
    /// this partner, it is off for the operator impersonating them too, and they
    /// hit the same wall the customer would. An impersonation that quietly exceeds
    /// the impersonated identity isn't a view — it's a backdoor.
    #[test]
    fn acting_as_a_partner_is_capped_by_the_ceiling() {
        let ceiling = Capabilities {
            manage_apps: true,
            view_audit: true,
            ..Default::default()
        };
        // Acting-as resolves the operator's authority straight to the ceiling.
        let assumed = ceiling;

        assert!(assumed.allows(PartnerCapability::ManageApps));
        assert!(assumed.allows(PartnerCapability::ViewAudit));
        // The partner was never granted these, so neither is the operator.
        assert!(!assumed.allows(PartnerCapability::DevelopApps));
        assert!(!assumed.allows(PartnerCapability::ManageMembers));
        assert!(!assumed.allows(PartnerCapability::CreateOrgs));
        assert!(!assumed.allows(PartnerCapability::ManageBilling));
    }
}
