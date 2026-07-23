//! `oxy-authz` — Oxy's **authorization** decision layer: one place that states who may
//! do what, so the answer is not re-derived at ~170 call sites. Authentication (who you
//! are) lives in `oxy-auth`; this is authorization (what you may do).
//!
//! Design: `internal-docs/2026-07-16-authorization-unification-design.md`.
//!
//! ## The model
//!
//! An [`Action`] is the closed vocabulary Oxy owns. Each maps to the authority `Ring`
//! that may perform it, and [`allows`] is the single `match` that states every ring.
//! Callers supply only FACTS — [`PrincipalFacts`], which orgs the principal
//! owns/admins/manages and the two global flags — plus a [`Resource`]. Loading those
//! facts is NOT here: it reaches app-specific primitives (org membership, partner
//! scope, the global-admin table), so it lives in `oxy-app`. This crate stays
//! transport-agnostic: it returns a bool or a [`Denied`], never an HTTP status.
//!
//! ## Why there is no policy engine
//!
//! This was built on Cedar and the engine was removed, because it earned nothing here:
//!
//! * The policy was generated from [`Action`] by string concatenation and parsed at
//!   runtime, so the one declarative, readable policy it was adopted for never existed
//!   as an artifact — what a reviewer read was Rust assembling policy text.
//! * The entity graph was degenerate. Every [`Resource`] carries its `org_id`, so the
//!   hierarchy only rediscovered what the caller had already supplied; every rule
//!   reduced to set containment, which is what [`allows`] does directly.
//! * `cedar validate` could not see the bug class this model actually hit: with every
//!   set typed `Set<Org>`, wiring the wrong capability into a ring type-checked clean.
//!   Verified — swapping `manage_apps_orgs` for `develop_apps_orgs` passed validation
//!   and was caught only by the behavioural tests.
//! * Policy-as-data (external authors, per-tenant roles) is an explicit non-goal
//!   (design §2) — and that is the requirement that pays for an engine.
//!
//! The value was never the engine: it was stating the model explicitly and DIFFERENCING
//! it against the shipped checks. That found every real bug here (billing excluded the
//! override; `develop_apps` is not `manage_apps`; operator reach must not out-rank a
//! real membership). Those tests survive; the runtime did not. Adopt an engine when
//! policy must be authored as data — not to compute `contains`.
//!
//! Fails **closed**: an unmapped combination simply has no arm that grants.

use uuid::Uuid;

/// An action a principal may take on a resource. Oxy owns this closed vocabulary, and
/// [`allows`] must have an arm for every variant — adding one here without giving it a
/// `Ring` fails the build, which is the exhaustiveness guarantee this enum exists for.
/// Grow it as families migrate.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Read anything in an org — any member.
    OrgRead,
    /// Invite / set-role / remove a member — org owner/admin, a managing partner,
    /// or global admin/owner.
    MemberInvite,
    MemberSetRole,
    MemberRemove,
    /// Billing (Stripe portal / invoices / checkout) — a **real** org owner or
    /// admin. Mirrors the `OrgAdminStrict` guard: unlike member management, the
    /// cross-tenant global-operator override does NOT reach it (Oxy staff are
    /// deliberately barred from a tenant's billing), and neither does a partner.
    OrgBilling,
    /// Owner-exclusive org operations (delete, ownership transfer, owner-promotion)
    /// — the `OrgOwner` guard: a real org **owner**, or a global operator acting in
    /// via the synthetic-Owner override. A partner (assumes only Admin) cannot.
    OrgOwnerManage,
    /// A **real** member's read (billing-status banner, checkout verify) — the
    /// `OrgMemberStrict` guard: any real org member, but NOT the cross-tenant
    /// override (so a tenant's billing state never leaks to an operator).
    OrgReadStrict,
    /// Destructive / settings-changing workspace action (delete, settings,
    /// force-push) — the `WorkspaceAdmin` guard: effective workspace Owner/Admin.
    /// Org owner/admin reach it through the org→workspace hierarchy; a per-workspace
    /// override can elevate a plain member; a managing partner (assumes Admin) and a
    /// global operator reach it via the override path.
    WorkspaceManage,
    /// Edit workspace contents (commit / push / pull / file edit) — the
    /// `WorkspaceEditor` guard: anyone above Viewer. Every org member resolves to at
    /// least workspace Member (overrides only elevate), so this is effectively any
    /// member of the workspace's org, plus a managing partner or global operator.
    WorkspaceEdit,
    /// Access a customer app's DATA plane (`check_customer_app_gates`): a real member
    /// of the app's org, an Oxy global admin, or a partner operator whose ceiling
    /// grants `develop_apps`. Deliberately NOT the general "manage" partner path —
    /// develop_apps is the read-the-app's-data capability, distinct from manage_apps.
    ///
    /// When the app is **restricted** (`apps.visibility = 'members'`), plain org
    /// membership no longer suffices — the principal must hold an `app_members`
    /// row, or be an org officer (owner/admin) / staff / a develop_apps partner.
    /// That is the one place in this model where a fact SUBTRACTS reach rather
    /// than adding it.
    AppAccess,
    /// Administer a customer app from *inside* the app — its privileged surface
    /// (e.g. the warehouse app's `?view=admin` and the data behind it). Any org
    /// **officer** (owner or admin), an `app_members` row with `role = 'admin'`,
    /// or Oxy staff.
    ///
    /// The `app_members` admin role extends admin rights DOWNWARD: it names an
    /// app's administrator who is NOT an org officer (the warehouse admin who
    /// isn't org staff), without granting org-wide billing/member management.
    /// Deliberately not a develop_apps partner — building an app is not
    /// administering its live privileged surface.
    AppAdmin,
    /// Rename a workspace (`ensure_org_admin_or_workspace_creator`): an org
    /// owner/admin, OR the plain member who CREATED that workspace. The creator half
    /// is the only place a `created_by` self-claim grants a workspace action, so it
    /// gets its own ring rather than widening the general self rule.
    WorkspaceRename,
    /// Delete a git namespace (`github::namespaces`): an org owner/admin, OR the
    /// member who created it. Same shape as [`Action::WorkspaceRename`] — org admin or the
    /// creator — which is why they share a ring instead of each re-deriving it.
    NamespaceDelete,
    /// Flip a workspace's Oxy-access switch (`workspace_oxy_access`): a **real**
    /// workspace owner/admin. The global-operator override is rejected on purpose —
    /// staff must not be able to self-grant access to a tenant's workspace.
    WorkspaceOxyAccess,

    // ── Partner ceiling (one action per PartnerCapability) ────────────────────
    // The partner tier shipped a whole second decision engine of its own. These absorb
    // it so there is ONE place that states the entire authority model. Each is "the
    // partner I am acting as holds <capability> over this client".
    PartnerManageMembers,
    PartnerManageApps,
    PartnerDevelopApps,
    PartnerViewAudit,
    PartnerManageBilling,
    PartnerManageSecrets,
    PartnerCreateOrgs,
    PartnerManageOrgSettings,

    // ── Platform (Oxy's own operator surfaces) ────────────────────────────────
    // These are not org-scoped: they target the `Platform` singleton. They are here
    // so the policy states the WHOLE authority model — a reader or auditor sees the
    // platform tier next to the tenant tiers instead of having to go read three
    // middlewares. They fuse no facts (each is one global flag), so they add no
    // decision power; they buy one place to read.
    /// Any Oxy operator surface (`oxy_owner_or_app_admin_guard`): a global admin OR a
    /// global owner. Both tiers reach everything — the admin console, the customer-app
    /// lifecycle, all of it. There is deliberately no admin-ONLY action: the tiers
    /// separate at [`Action::PlatformOwnerOnly`], not before it.
    PlatformOps,
    /// The owner-exclusive surfaces — and the ONLY place the two operator tiers differ:
    /// destructive or irreversible operations (deleting the master org, demoting other
    /// admins), plus the Billing queue. A global **owner** only.
    PlatformOwnerOnly,
}

impl Action {
    pub const ALL: [Action; 24] = [
        Action::OrgRead,
        Action::MemberInvite,
        Action::MemberSetRole,
        Action::MemberRemove,
        Action::OrgBilling,
        Action::OrgOwnerManage,
        Action::OrgReadStrict,
        Action::WorkspaceManage,
        Action::WorkspaceEdit,
        Action::AppAccess,
        Action::AppAdmin,
        Action::WorkspaceRename,
        Action::NamespaceDelete,
        Action::WorkspaceOxyAccess,
        Action::PartnerManageMembers,
        Action::PartnerManageApps,
        Action::PartnerDevelopApps,
        Action::PartnerViewAudit,
        Action::PartnerManageBilling,
        Action::PartnerManageSecrets,
        Action::PartnerCreateOrgs,
        Action::PartnerManageOrgSettings,
        Action::PlatformOps,
        Action::PlatformOwnerOnly,
    ];

    /// Stable log id. This is what appears in the `authz` tracing output, so treat it
    /// as a wire contract: renaming a variant is free, renaming its id breaks whatever
    /// is grepping the logs.
    fn as_str(self) -> &'static str {
        match self {
            Action::OrgRead => "org_read",
            Action::MemberInvite => "member_invite",
            Action::MemberSetRole => "member_set_role",
            Action::MemberRemove => "member_remove",
            Action::OrgBilling => "org_billing",
            Action::OrgOwnerManage => "org_owner_manage",
            Action::OrgReadStrict => "org_read_strict",
            Action::WorkspaceManage => "workspace_manage",
            Action::WorkspaceEdit => "workspace_edit",
            Action::AppAccess => "app_access",
            Action::AppAdmin => "app_admin",
            Action::WorkspaceRename => "workspace_rename",
            Action::NamespaceDelete => "namespace_delete",
            Action::WorkspaceOxyAccess => "workspace_oxy_access",
            // Ids match PartnerCapability::as_str, prefixed — the partner tier's own
            // policy uses the bare cap name; these live in the shared action space.
            Action::PartnerManageMembers => "partner_manage_members",
            Action::PartnerManageApps => "partner_manage_apps",
            Action::PartnerDevelopApps => "partner_develop_apps",
            Action::PartnerViewAudit => "partner_view_audit",
            Action::PartnerManageBilling => "partner_manage_billing",
            Action::PartnerManageSecrets => "partner_manage_secrets",
            Action::PartnerCreateOrgs => "partner_create_orgs",
            Action::PartnerManageOrgSettings => "partner_manage_org_settings",
            Action::PlatformOps => "platform_ops",
            Action::PlatformOwnerOnly => "platform_owner_only",
        }
    }

    /// Which authority ring the action requires.
    fn ring(self) -> Ring {
        match self {
            Action::OrgRead => Ring::Read,
            Action::MemberInvite | Action::MemberSetRole | Action::MemberRemove => Ring::OrgAdmin,
            Action::OrgBilling => Ring::OrgAdminStrict,
            Action::OrgOwnerManage => Ring::OwnerOnly,
            Action::OrgReadStrict => Ring::MemberStrict,
            Action::WorkspaceManage => Ring::WorkspaceAdmin,
            Action::WorkspaceEdit => Ring::WorkspaceEdit,
            Action::AppAccess => Ring::AppAccess,
            Action::AppAdmin => Ring::AppAdmin,
            Action::WorkspaceRename | Action::NamespaceDelete => Ring::OrgAdminOrCreator,
            Action::WorkspaceOxyAccess => Ring::WorkspaceAdminStrict,
            Action::PartnerManageMembers => Ring::PartnerCap(Cap::ManageMembers),
            Action::PartnerManageApps => Ring::PartnerCap(Cap::ManageApps),
            Action::PartnerDevelopApps => Ring::PartnerCap(Cap::DevelopApps),
            Action::PartnerViewAudit => Ring::PartnerCap(Cap::ViewAudit),
            Action::PartnerManageBilling => Ring::PartnerCap(Cap::ManageBilling),
            Action::PartnerManageSecrets => Ring::PartnerCap(Cap::ManageSecrets),
            Action::PartnerCreateOrgs => Ring::PartnerCap(Cap::CreateOrgs),
            Action::PartnerManageOrgSettings => Ring::PartnerCap(Cap::ManageOrgSettings),
            Action::PlatformOps => Ring::GlobalAdminOrOwner,
            Action::PlatformOwnerOnly => Ring::GlobalOwnerOnly,
        }
    }
}

/// A partner ceiling capability, one-to-one with `PartnerCapability`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Cap {
    ManageMembers,
    ManageApps,
    DevelopApps,
    ViewAudit,
    ManageBilling,
    ManageSecrets,
    CreateOrgs,
    ManageOrgSettings,
}

impl Cap {
    /// Every capability — the full ceiling.
    pub const ALL: [Cap; 8] = [
        Cap::ManageMembers,
        Cap::ManageApps,
        Cap::DevelopApps,
        Cap::ViewAudit,
        Cap::ManageBilling,
        Cap::ManageSecrets,
        Cap::CreateOrgs,
        Cap::ManageOrgSettings,
    ];
}

impl PrincipalFacts {
    /// Any partner the principal operates that manages `org_id` — the coarse "operates
    /// this client", used where the shipped check is the override path rather than a
    /// specific capability.
    fn manages(&self, org_id: Uuid) -> bool {
        self.partners
            .iter()
            .any(|p| p.client_orgs.contains(&org_id))
    }

    /// A partner the principal operates that manages `org_id` AND holds `cap`. Not
    /// scoped to an acting partner: used where the shipped check resolves the partner
    /// FROM the org (the customer-app data plane), not from the URL.
    fn any_partner_grants(&self, cap: Cap, org_id: Uuid) -> bool {
        self.partners.iter().any(|p| p.grants(cap, org_id))
    }
}

/// The authority rings that gate an action (self is handled separately, on
/// `resource.owner`). The difference between [`Ring::OrgAdmin`] and
/// [`Ring::OrgAdminStrict`] is the global-operator override: member management honors
/// it, billing does not. The two workspace rings ride the org→workspace hierarchy (a
/// workspace is a child of its org), so an org role governs its workspaces for free;
/// [`Ring::WorkspaceAdmin`] adds the per-workspace elevation override.
///
/// Deliberately private: a ring is how the model is *stated*, not a vocabulary callers
/// choose from. Callers name an [`Action`] — the thing they are actually doing — and the
/// mapping to a ring is this crate's to decide. Making it public would let a call site
/// pick its own authority level, which is the scatter this crate exists to end.
#[derive(Copy, Clone)]
enum Ring {
    /// Any member of the resource's org, or a global admin/owner.
    Read,
    /// A **real** member of the org — no global override (the `OrgMemberStrict` gate,
    /// so a tenant's member-only reads don't leak to a cross-tenant operator).
    MemberStrict,
    /// A real org owner/admin, a managing partner, OR a global admin/owner reaching
    /// in via the cross-tenant override.
    OrgAdmin,
    /// A **real** org owner/admin only — the global-operator override and partners
    /// are excluded (the `OrgAdminStrict` billing gate).
    OrgAdminStrict,
    /// A real org **owner**, or a global operator via the synthetic-Owner override
    /// (which resolves to Owner). A partner assumes only Admin, so it's excluded
    /// (the `OrgOwner` gate).
    OwnerOnly,
    /// Effective workspace Owner/Admin: org owner/admin (via the hierarchy), a
    /// per-workspace override that elevates a member, a managing partner, or a global
    /// operator (the `WorkspaceAdmin` guard).
    WorkspaceAdmin,
    /// Effective workspace Member or above: any member of the workspace's org (every
    /// org member resolves to ≥ Member; overrides only elevate), a managing partner,
    /// or a global operator (the `WorkspaceEditor` guard).
    WorkspaceEdit,
    /// Customer-app data plane: a real member of the app's org, a global admin, or a
    /// partner with `develop_apps` over the org (`check_customer_app_gates`). NOT the
    /// coarse managed-partner path, and NOT global owner (the check uses app-admin).
    ///
    /// Conditional on `resource.app_restricted`: a restricted app drops the plain
    /// org-membership term and demands an `app_members` row (org officers + staff +
    /// develop_apps partner remain).
    AppAccess,
    /// A customer app's own privileged surface: any org officer (owner/admin), an
    /// `app_members` admin row, or Oxy staff. No partner term — see
    /// [`Action::AppAdmin`].
    AppAdmin,
    /// Org owner/admin (via the hierarchy) OR the thing's creator — the plain member
    /// who made it may manage it. The creator claim reads `resource.owner`
    /// (`created_by`), and is confined to the actions that opt into this ring, so it
    /// can never leak into a privileged one.
    OrgAdminOrCreator,
    /// A **real** workspace owner/admin — the global-operator override is rejected, so
    /// no Oxy operator (and no partner, which assumes via the same override) can flip
    /// it. The workspace analogue of [`OrgAdminStrict`]; gates the Oxy-access switch,
    /// which only a real org officer may change.
    WorkspaceAdminStrict,
    /// A partner ceiling capability over the resource's org — `client ∈ partner.clients
    /// && cap ∈ partner.caps`, where the partner is the one being **acted as**, not any
    /// partner the principal happens to hold standing through.
    ///
    /// No global or membership term, deliberately: the ceiling is the whole story, so
    /// standing elsewhere cannot widen it.
    PartnerCap(Cap),
    /// Platform tier — a global admin OR owner (`oxy_owner_or_app_admin_guard`).
    GlobalAdminOrOwner,
    /// Platform tier — a global **owner** only (Billing queue, global-admin mgmt).
    GlobalOwnerOnly,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Org,
    Workspace,
    App,
    Thread,
    /// A git namespace, a child of its org.
    Namespace,
    /// A partner org itself — the target of a capability-only console check (one with
    /// no client org yet, e.g. "may this partner create orgs").
    Partner,
    /// Oxy itself — the singleton target of the platform tier. Not org-scoped, and
    /// deliberately parented to nothing, so no org set can ever reach it.
    Platform,
}

/// The resource an action targets. Every resource belongs to an org (an `Org`
/// resource is its own scope); `owner` is set for self-owned resources (threads),
/// enabling the self ring.
#[derive(Clone, Debug)]
pub struct Resource {
    pub kind: ResourceKind,
    pub id: Uuid,
    /// The org the resource belongs to. For an `Org` resource this equals `id`.
    pub org_id: Uuid,
    pub owner: Option<Uuid>,
    /// The partner being ACTED AS, for partner-console decisions. The console is scoped
    /// to one partner by its URL, and that scope is part of the decision: holding a
    /// capability through partner B must not authorize anything while scoped to A.
    /// `None` for every non-partner decision.
    pub partner: Option<Uuid>,
    /// For an `App` resource: the app is restricted to explicitly-listed members
    /// (`apps.visibility = 'members'`). `false` — the default everywhere else —
    /// preserves the historical "any org member" rule, so this can only ever
    /// tighten a specific app and never loosens one.
    pub app_restricted: bool,
}

impl Resource {
    /// An org itself (member management, org settings).
    pub fn org(org_id: Uuid) -> Self {
        Self {
            kind: ResourceKind::Org,
            id: org_id,
            org_id,
            owner: None,
            partner: None,
            app_restricted: false,
        }
    }

    /// A workspace, scoped to the org it belongs to (so the org→workspace hierarchy
    /// resolves). Pass the org that owns the workspace as `org_id`.
    pub fn workspace(id: Uuid, org_id: Uuid) -> Self {
        Self {
            kind: ResourceKind::Workspace,
            id,
            org_id,
            owner: None,
            partner: None,
            app_restricted: false,
        }
    }

    /// Oxy itself — the singleton the platform tier targets (the admin console, the
    /// Billing queue, global-admin management). It has no org: `org_id` is nil and no
    /// platform ring reads an org set, so nothing tenant-scoped can reach it and it
    /// can't reach anything tenant-scoped.
    pub fn platform() -> Self {
        Self {
            kind: ResourceKind::Platform,
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            owner: None,
            partner: None,
            app_restricted: false,
        }
    }

    /// A partner console decision with no client org — "does the partner I am acting as
    /// hold this capability at all" (e.g. create-orgs, view-audit).
    pub fn partner(partner_id: Uuid) -> Self {
        Self {
            kind: ResourceKind::Partner,
            id: partner_id,
            org_id: Uuid::nil(),
            owner: None,
            partner: Some(partner_id),
            app_restricted: false,
        }
    }

    /// A client org, reached while ACTING AS `acting_partner`. Both halves matter: the
    /// capability must come from that partner, and the org must be its client.
    pub fn partner_client(client_org_id: Uuid, acting_partner: Uuid) -> Self {
        Self {
            kind: ResourceKind::Org,
            id: client_org_id,
            org_id: client_org_id,
            owner: None,
            partner: Some(acting_partner),
            app_restricted: false,
        }
    }

    /// A workspace carrying its creator (`created_by`) as `owner`, for the one ring
    /// that honours a creator claim ([`Action::WorkspaceRename`]). Every other
    /// workspace ring ignores `owner`, so this can't widen them.
    pub fn workspace_with_creator(id: Uuid, org_id: Uuid, created_by: Option<Uuid>) -> Self {
        Self {
            kind: ResourceKind::Workspace,
            id,
            org_id,
            owner: created_by,
            partner: None,
            app_restricted: false,
        }
    }

    /// A git namespace carrying its creator as `owner`, scoped to its org — for the
    /// org-admin-or-creator ring.
    pub fn namespace_with_creator(id: Uuid, org_id: Uuid, created_by: Option<Uuid>) -> Self {
        Self {
            kind: ResourceKind::Namespace,
            id,
            org_id,
            owner: created_by,
            partner: None,
            app_restricted: false,
        }
    }

    /// A customer app, scoped to its owning org (the app is a child of the org, so
    /// `resource in <org set>` resolves). `id` may be the app/project id.
    ///
    /// Treated as **unrestricted** (`visibility = 'org'`) — the historical rule. Use
    /// [`Resource::app_with_visibility`] where the app row is in hand, so a
    /// restricted app is actually enforced.
    pub fn app(id: Uuid, org_id: Uuid) -> Self {
        Self {
            kind: ResourceKind::App,
            id,
            org_id,
            owner: None,
            partner: None,
            app_restricted: false,
        }
    }

    /// A customer app carrying its visibility. `id` MUST be the **app id** (not the
    /// project/workspace id): per-app membership is keyed by it, so passing the
    /// wrong id would silently deny every member of a restricted app.
    pub fn app_with_visibility(id: Uuid, org_id: Uuid, restricted: bool) -> Self {
        Self {
            app_restricted: restricted,
            ..Self::app(id, org_id)
        }
    }
}

/// A principal's authorization-relevant facts, loaded once per request by the app-side
/// loader. This is the crate's whole input surface: the loader states what is TRUE of
/// the principal, [`allows`] decides what that entitles them to. Keeping those two
/// apart is what lets the model be tested without a database.
///
/// Empty = a plain authenticated user with no org role and no global flag → denied
/// everything (fail closed).
#[derive(Clone, Debug, Default)]
pub struct PrincipalFacts {
    pub user_id: Uuid,
    /// Orgs where the user is **owner**.
    pub owned_orgs: Vec<Uuid>,
    /// Orgs where the user is owner **or** admin.
    pub admin_orgs: Vec<Uuid>,
    /// Every org the user is a member of (any role).
    pub member_orgs: Vec<Uuid>,
    /// Every partner this principal operates, each with its clients and its ceiling.
    ///
    /// This is deliberately NOT flattened into per-capability org sets. Flattening
    /// loses WHICH partner granted a capability, and the partner console is scoped to
    /// one acting partner (`/partners/{id}/...`) — so a flattened model answers "yes"
    /// for partner B's client while you are scoped to A, which is broader than the
    /// shipped check. Keeping the partner is what lets [`allows`] honour that scope.
    /// (The flat shape was a concession to the policy engine's `Set<Org>` attributes;
    /// without it, the model can just be correct.)
    pub partners: Vec<PartnerStanding>,
    /// Workspaces where a per-workspace `workspace_members` override raises the user
    /// to Admin or Owner above their org-derived role. Only the *exceptional* rows —
    /// the org-derived workspace role comes free from the org sets via the hierarchy,
    /// and overrides can only elevate, never downgrade.
    pub ws_admin_override: Vec<Uuid>,
    /// Apps where the principal holds an `app_members` row of ANY role. Read only
    /// by [`Ring::AppAccess`], and only when the app is restricted — so an app
    /// whose visibility is `org` behaves exactly as it did before this existed.
    pub app_memberships: Vec<Uuid>,
    /// Apps where the principal's `app_members` row is `role = 'admin'`. A subset
    /// of [`Self::app_memberships`]; gates [`Ring::AppAdmin`].
    pub app_admin_memberships: Vec<Uuid>,
    pub is_global_admin: bool,
    pub is_global_owner: bool,
}

/// One partner the principal operates: its clients, and the ceiling over them.
#[derive(Clone, Debug)]
pub struct PartnerStanding {
    pub partner_id: Uuid,
    /// The client orgs this partner manages. Every operator reaches all of them.
    pub client_orgs: Vec<Uuid>,
    /// What Oxy granted this partner. The operator's authority IS the ceiling.
    pub caps: Vec<Cap>,
}

impl PartnerStanding {
    fn grants(&self, cap: Cap, org_id: Uuid) -> bool {
        self.caps.contains(&cap) && self.client_orgs.contains(&org_id)
    }
}

/// A denied authorization decision. The transport layer decides its HTTP shape (403
/// forbidden, or 404 to hide existence); this crate stays transport-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    pub action: Action,
}

/// The decision: pure set arithmetic over the facts. **This is the whole model** — every
/// authorization decision in Oxy resolves to one arm of the `match` below, so a rule that
/// is not stated here is not a rule.
///
/// This ran on a policy engine once; the crate header records why it earned nothing.
/// What is left is the same rules in the language the rest of the codebase is written in,
/// with exhaustiveness as the compiler's job: a new [`Action`] with no `Ring`, or a
/// `Ring` with no arm here, fails the build.
pub fn allows(facts: &PrincipalFacts, action: Action, resource: &Resource) -> bool {
    // Self: a thread's owner may READ it. Scoped to BOTH kind and action — unscoped,
    // this would hand the owner of any future owner-bearing resource every action.
    if action == Action::OrgRead
        && resource.kind == ResourceKind::Thread
        && resource.owner == Some(facts.user_id)
    {
        return true;
    }

    // A resource is reached through its org: `Resource::org` sets `org_id == id` and
    // every child carries its parent's, so one containment serves both. `Platform` has
    // a nil org and is therefore in no set — nothing tenant-scoped reaches it.
    let in_org = |set: &[Uuid]| set.contains(&resource.org_id);
    // The per-workspace elevation is keyed by the workspace, not its org.
    let elevated_here =
        resource.kind == ResourceKind::Workspace && facts.ws_admin_override.contains(&resource.id);
    let is_global = facts.is_global_admin || facts.is_global_owner;
    let is_platform = resource.kind == ResourceKind::Platform;

    // Operator reach — staff, and a managing partner — models the SYNTHETIC-OWNER
    // override, which the middleware applies only when the caller is NOT a real member.
    // Unconditional, it out-ranks a real membership and silently promotes an operator
    // who happens to be a plain member of the tenant.
    let not_member = !in_org(&facts.member_orgs);
    let operator = not_member && is_global;
    let operator_or_partner = not_member && (is_global || facts.manages(resource.org_id));

    match action.ring() {
        Ring::Read => in_org(&facts.member_orgs) || operator,
        Ring::MemberStrict => in_org(&facts.member_orgs),
        Ring::OrgAdmin => in_org(&facts.admin_orgs) || operator_or_partner,
        // Billing: real owner/admin only — the override is barred and partners don't bill.
        Ring::OrgAdminStrict => in_org(&facts.admin_orgs),
        // A partner assumes Admin, never Owner, so no partner term here.
        Ring::OwnerOnly => in_org(&facts.owned_orgs) || operator,
        Ring::OrgAdminOrCreator => {
            in_org(&facts.admin_orgs)
                || resource.owner == Some(facts.user_id)
                || operator_or_partner
        }
        Ring::WorkspaceAdmin => in_org(&facts.admin_orgs) || elevated_here || operator_or_partner,
        // The Oxy-access switch: a REAL workspace officer; the override is rejected so
        // staff cannot unlock themselves.
        Ring::WorkspaceAdminStrict => in_org(&facts.admin_orgs) || elevated_here,
        Ring::WorkspaceEdit => in_org(&facts.member_orgs) || operator_or_partner,
        // A member of the app's org, any Oxy operator, or a develop_apps partner.
        //
        // The operator term is deliberately NOT conditioned on non-membership the way
        // the org rings are: this gate really does grant staff unconditionally, and it
        // is the same answer either way — a staff member of the org already passes on
        // the membership term.
        //
        // The partner term resolves FROM the org, not from a console URL, so it is not
        // scoped to an acting partner. And it is develop_apps, never the coarse managed
        // path: reading an app's data is not managing its lifecycle.
        Ring::AppAccess => {
            // Break-glass regardless of visibility: staff, a develop_apps partner,
            // and any org OFFICER (owner or admin — `admin_orgs` contains owners).
            // An org's own officers are never locked out of its apps.
            let unconditional = is_global
                || facts.any_partner_grants(Cap::DevelopApps, resource.org_id)
                || in_org(&facts.admin_orgs);
            if resource.app_restricted {
                // Restricted: plain org membership is NOT enough — that is the
                // whole point. An explicit `app_members` row (either role) is.
                unconditional || facts.app_memberships.contains(&resource.id)
            } else {
                // Unrestricted (the default): unchanged — any member of the org.
                unconditional || in_org(&facts.member_orgs)
            }
        }
        // An org officer (owner or admin) administers every app in the org. The
        // `app_members` admin role extends that DOWNWARD to a non-officer — the
        // app's designated admin who isn't org staff (e.g. the warehouse admin) —
        // without granting org-wide billing/member powers. No develop_apps term:
        // building an app is not administering its live privileged surface.
        Ring::AppAdmin => {
            is_global
                || in_org(&facts.admin_orgs)
                || facts.app_admin_memberships.contains(&resource.id)
        }
        // The console IS scoped: the capability must come from the partner being acted
        // as. Operating a partner that grants `cap` over some other client authorizes
        // nothing here — that scope is what a flattened model silently dropped.
        Ring::PartnerCap(cap) => match resource.partner {
            None => false, // a partner decision must name the partner it is acting as
            Some(acting) => facts.partners.iter().any(|p| {
                p.partner_id == acting
                    && p.caps.contains(&cap)
                    && (resource.kind == ResourceKind::Partner
                        || p.client_orgs.contains(&resource.org_id))
            }),
        },
        // The platform tier reads ONLY the global flags and is pinned to the Platform
        // singleton: no tenant standing reaches an operator surface, and no platform
        // action can be asked of a tenant resource.
        Ring::GlobalAdminOrOwner => is_platform && is_global,
        Ring::GlobalOwnerOnly => is_platform && facts.is_global_owner,
    }
}

/// Enforce: `Ok(())` on allow, [`Denied`] on deny. The **end state** — the model is the
/// sole authority, with no legacy term beside it. Reach for this only where a call site
/// has no shipped check to difference against (a new surface), or where its legacy check
/// has been retired. Everywhere else, [`enforce`] keeps the old verdict as a fail-safe.
/// The caller maps [`Denied`] to its HTTP status (403 by default).
pub fn require(facts: &PrincipalFacts, action: Action, resource: &Resource) -> Result<(), Denied> {
    if allows(facts, action, resource) {
        Ok(())
    } else {
        Err(Denied { action })
    }
}

/// **The normal entry point.** The decision is `existing_allow && allows(..)`. The model
/// is binding — a deny here is a real 403 — but it sits beside the check it replaced
/// rather than on top of it.
///
/// The conjunction is deliberate and is the whole safety property: the model can only
/// ever REMOVE access the existing check already granted, so a mis-modeled ring **cannot
/// open a hole** (no cross-tenant grant is reachable through here). The residual failure
/// mode is a wrong DENY, which is loud (an immediate 403), attributable (a WARN naming
/// the label), and revertible in one line.
///
/// `existing_allow` is not ceremony — it is the **oracle**. It is the shipped check the
/// differential tests difference the model against, which is what caught every real bug
/// in this model. Passing a hand-waved `true` here silently converts a fail-safe into a
/// bare `allows` and throws that away.
///
/// This is a migration step. When a ring's legacy term is retired, call [`require`]
/// instead and delete the old branch.
pub fn enforce(
    label: &str,
    facts: &PrincipalFacts,
    action: Action,
    resource: &Resource,
    existing_allow: bool,
) -> bool {
    let unified_allow = allows(facts, action, resource);
    if unified_allow != existing_allow {
        // Enforcing, so this is not a curiosity: where the legacy check allowed and
        // the model denies, a real request just took a 403 attributable to this label.
        tracing::warn!(
            target: "authz",
            label,
            action = action.as_str(),
            user = %facts.user_id,
            org = %resource.org_id,
            unified_allow,
            existing_allow,
            newly_denied = existing_allow && !unified_allow,
            "authz disagreement — the unified model diverged from the legacy check; \
             the conjunction denies. Investigate the ring."
        );
    }
    // Can only subtract. A hole is unreachable; a wrong deny is visible above.
    existing_allow && unified_allow
}

/// **Authoritative mode.** The model DECIDES; the legacy verdict is kept only as an
/// OBSERVER. The inverse of [`enforce`]: the model's verdict is returned, and
/// `legacy_observed` is compared to it purely to keep the disagreement signal alive.
///
/// This drops the fail-safe conjunction, so a mis-modeled ring CAN open a hole — the one
/// failure [`enforce`] makes unreachable. Use it only where differential tests prove the
/// ring matches its oracle across the scenario space, and prefer it over [`require`] only
/// when you still want the legacy signal on real traffic.
///
/// **Currently unwired**, deliberately: no call site has passed a seeded-database
/// differential run yet, and dropping a fail-safe on the strength of hand-built facts
/// would be trusting an assumption about the loader rather than the loader.
///
/// When the observer is no longer wanted, call [`require`] — the legacy branch is then
/// dead and can be deleted.
pub fn authorize(
    label: &str,
    facts: &PrincipalFacts,
    action: Action,
    resource: &Resource,
    legacy_observed: bool,
) -> bool {
    let unified_allow = allows(facts, action, resource);
    if unified_allow != legacy_observed {
        tracing::warn!(
            target: "authz",
            label,
            action = action.as_str(),
            user = %facts.user_id,
            org = %resource.org_id,
            unified_allow,
            legacy_observed,
            authority = "unified",
            "authz disagreement — the unified model decided; the legacy check would have \
             differed. The model's answer stands; investigate the ring."
        );
    }
    unified_allow
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    fn org() -> Uuid {
        Uuid::from_u128(1)
    }
    fn other_org() -> Uuid {
        Uuid::from_u128(2)
    }
    fn user() -> Uuid {
        Uuid::from_u128(100)
    }

    /// A partner standing: `p(client, &[caps])` — operated partner id defaults to
    /// `partner_org()` unless given.
    fn standing(partner_id: Uuid, client: Uuid, caps: &[Cap]) -> PartnerStanding {
        PartnerStanding {
            partner_id,
            client_orgs: vec![client],
            caps: caps.to_vec(),
        }
    }
    fn partner_org() -> Uuid {
        Uuid::from_u128(900)
    }

    fn facts() -> PrincipalFacts {
        PrincipalFacts {
            user_id: user(),
            ..Default::default()
        }
    }

    #[test]
    fn org_admin_may_manage_members() {
        let f = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(allows(&f, Action::MemberSetRole, &Resource::org(org())));
        assert!(allows(&f, Action::OrgRead, &Resource::org(org())));
    }

    #[test]
    fn global_flags_must_not_grant_over_a_real_membership() {
        // An Oxy staffer who is ALSO a plain member of a customer org. The legacy
        // guard denies: org_context hands them their REAL row (role = Member,
        // is_global_override = false), so `matches!(role, Owner|Admin)` is false.
        //
        // The global flags model the SYNTHETIC-OWNER override path — which only
        // applies when the operator is NOT a real member. As unconditional facts they
        // would fire here too and hand a plain member org-admin: a privilege
        // escalation, live the moment the legacy term is dropped.
        let staffer_who_is_a_plain_member = PrincipalFacts {
            member_orgs: vec![org()],
            is_global_admin: true,
            ..facts()
        };
        assert!(!allows(
            &staffer_who_is_a_plain_member,
            Action::MemberSetRole,
            &Resource::org(org())
        ));
        // Same for a global owner.
        let owner_who_is_a_plain_member = PrincipalFacts {
            member_orgs: vec![org()],
            is_global_owner: true,
            ..facts()
        };
        assert!(!allows(
            &owner_who_is_a_plain_member,
            Action::MemberSetRole,
            &Resource::org(org())
        ));
        // The override path itself must still work: NOT a real member -> the operator
        // reaches in (org_context synthesizes Owner, and the legacy check agrees).
        let staffer_not_a_member = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        assert!(allows(
            &staffer_not_a_member,
            Action::MemberSetRole,
            &Resource::org(org())
        ));
    }

    #[test]
    fn plain_member_reads_but_cannot_manage() {
        let f = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(allows(&f, Action::OrgRead, &Resource::org(org())));
        assert!(!allows(&f, Action::MemberSetRole, &Resource::org(org())));
        assert!(!allows(&f, Action::OrgBilling, &Resource::org(org())));
    }

    #[test]
    fn billing_admits_real_owner_and_admin_but_not_override_or_partner() {
        // OrgAdminStrict: a REAL owner or admin bills; the cross-tenant global
        // override and partners do not.
        let real_owner = PrincipalFacts {
            owned_orgs: vec![org()],
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        let real_admin = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        // A global operator who is NOT a real member reaches the org only via the
        // synthetic-Owner override, which billing rejects.
        let global_only = PrincipalFacts {
            is_global_admin: true,
            is_global_owner: true,
            ..facts()
        };
        let partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        assert!(allows(
            &real_owner,
            Action::OrgBilling,
            &Resource::org(org())
        ));
        assert!(allows(
            &real_admin,
            Action::OrgBilling,
            &Resource::org(org())
        ));
        assert!(!allows(
            &global_only,
            Action::OrgBilling,
            &Resource::org(org())
        ));
        assert!(!allows(&partner, Action::OrgBilling, &Resource::org(org())));
    }

    #[test]
    fn partner_may_manage_a_managed_org() {
        let f = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        assert!(allows(&f, Action::MemberSetRole, &Resource::org(org())));
        // ...but not billing (partner ceiling doesn't reach OrgAdminStrict).
        assert!(!allows(&f, Action::OrgBilling, &Resource::org(org())));
    }

    #[test]
    fn global_reaches_member_mgmt_via_override_but_not_billing() {
        let ga = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        let go = PrincipalFacts {
            is_global_owner: true,
            ..facts()
        };
        // Member management honors the cross-tenant override — both globals pass.
        assert!(allows(&ga, Action::MemberSetRole, &Resource::org(org())));
        assert!(allows(&go, Action::MemberSetRole, &Resource::org(org())));
        // Billing (OrgAdminStrict) rejects the override — neither global reaches it.
        assert!(!allows(&ga, Action::OrgBilling, &Resource::org(org())));
        assert!(!allows(&go, Action::OrgBilling, &Resource::org(org())));
    }

    #[test]
    fn wrong_org_and_outsider_are_denied() {
        // Admin of a DIFFERENT org.
        let elsewhere = PrincipalFacts {
            admin_orgs: vec![other_org()],
            member_orgs: vec![other_org()],
            ..facts()
        };
        assert!(!allows(
            &elsewhere,
            Action::MemberSetRole,
            &Resource::org(org())
        ));
        assert!(!allows(&elsewhere, Action::OrgRead, &Resource::org(org())));
        // A plain authenticated user with no facts.
        assert!(!allows(&facts(), Action::OrgRead, &Resource::org(org())));
    }

    #[test]
    fn self_owned_resource_is_reachable_by_its_owner() {
        let f = facts();
        let thread = Resource {
            kind: ResourceKind::Thread,
            id: Uuid::from_u128(500),
            org_id: org(),
            owner: Some(user()),
            partner: None,
            app_restricted: false,
        };
        assert!(allows(&f, Action::OrgRead, &thread));
        // A different user does not own it.
        let other = PrincipalFacts {
            user_id: Uuid::from_u128(101),
            ..Default::default()
        };
        assert!(!allows(&other, Action::OrgRead, &thread));
    }

    #[test]
    fn self_rule_grants_only_reads_and_only_to_owner_bearing_threads() {
        // Pins the scope of the self rule, so a future owner-bearing resource (or a
        // privileged action on an owned thread) can never ride it into an allow once
        // rings flip to enforce.
        let f = facts();
        let thread = Resource {
            kind: ResourceKind::Thread,
            id: Uuid::from_u128(500),
            org_id: org(),
            owner: Some(user()),
            partner: None,
            app_restricted: false,
        };
        // Owning a thread grants the read...
        assert!(allows(&f, Action::OrgRead, &thread));
        // ...and NEVER a privileged action, even on the resource you own.
        assert!(!allows(&f, Action::OrgBilling, &thread));
        assert!(!allows(&f, Action::MemberSetRole, &thread));
        assert!(!allows(&f, Action::WorkspaceManage, &thread));
        assert!(!allows(&f, Action::OrgOwnerManage, &thread));

        // An owner-bearing resource of a DIFFERENT kind gets no self grant at all —
        // this is the case that would otherwise become a hole.
        let owned_workspace = Resource {
            kind: ResourceKind::Workspace,
            id: Uuid::from_u128(701),
            org_id: org(),
            owner: Some(user()),
            partner: None,
            app_restricted: false,
        };
        assert!(!allows(&f, Action::OrgRead, &owned_workspace));
        assert!(!allows(&f, Action::WorkspaceManage, &owned_workspace));
        assert!(!allows(&f, Action::WorkspaceEdit, &owned_workspace));
    }

    #[test]
    fn workspace_manage_needs_admin_or_per_workspace_override() {
        let ws = Uuid::from_u128(700);
        let r = Resource::workspace(ws, org());
        // Org owner/admin govern the workspace through the org hierarchy.
        let org_admin = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(allows(&org_admin, Action::WorkspaceManage, &r));
        // A plain org member does NOT manage it...
        let member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(!allows(&member, Action::WorkspaceManage, &r));
        // ...unless a per-workspace override elevates them on THIS workspace.
        let elevated = PrincipalFacts {
            member_orgs: vec![org()],
            ws_admin_override: vec![ws],
            ..facts()
        };
        assert!(allows(&elevated, Action::WorkspaceManage, &r));
        // An override on a different workspace doesn't help.
        let elsewhere = PrincipalFacts {
            member_orgs: vec![org()],
            ws_admin_override: vec![Uuid::from_u128(701)],
            ..facts()
        };
        assert!(!allows(&elsewhere, Action::WorkspaceManage, &r));
        // A managing partner (assumes Admin) reaches it.
        let partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        assert!(allows(&partner, Action::WorkspaceManage, &r));
    }

    #[test]
    fn workspace_edit_is_any_member_partner_or_global() {
        let ws = Uuid::from_u128(700);
        let r = Resource::workspace(ws, org());
        let member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        let partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        let global = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        let outsider = PrincipalFacts {
            member_orgs: vec![other_org()],
            ..facts()
        };
        assert!(allows(&member, Action::WorkspaceEdit, &r));
        assert!(allows(&partner, Action::WorkspaceEdit, &r));
        assert!(allows(&global, Action::WorkspaceEdit, &r));
        assert!(!allows(&outsider, Action::WorkspaceEdit, &r));
    }

    #[test]
    fn owner_only_is_real_owner_or_global_not_admin_or_partner() {
        let r = Resource::org(org());
        let owner = PrincipalFacts {
            owned_orgs: vec![org()],
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        // A real admin is NOT an owner.
        let admin = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        // A partner assumes Admin, never Owner.
        let partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        // A global operator reaches in as the synthetic Owner.
        let global = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        assert!(allows(&owner, Action::OrgOwnerManage, &r));
        assert!(!allows(&admin, Action::OrgOwnerManage, &r));
        assert!(!allows(&partner, Action::OrgOwnerManage, &r));
        assert!(allows(&global, Action::OrgOwnerManage, &r));
    }

    #[test]
    fn member_strict_is_real_member_only_no_global_or_partner() {
        let r = Resource::org(org());
        let member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        // A global operator with no real membership must NOT get a member-strict read
        // (billing-status/checkout must not leak cross-tenant).
        let global = PrincipalFacts {
            is_global_admin: true,
            is_global_owner: true,
            ..facts()
        };
        let partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        assert!(allows(&member, Action::OrgReadStrict, &r));
        assert!(!allows(&global, Action::OrgReadStrict, &r));
        assert!(!allows(&partner, Action::OrgReadStrict, &r));
    }

    #[test]
    fn app_access_is_member_global_admin_or_develop_apps_partner() {
        let app = Resource::app(Uuid::from_u128(900), org());
        // A real member of the app's org.
        let member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        // An Oxy global admin.
        let ga = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        // A partner whose ceiling grants develop_apps over the org.
        let dev_partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &[Cap::DevelopApps])],
            ..facts()
        };
        assert!(allows(&member, Action::AppAccess, &app));
        assert!(allows(&ga, Action::AppAccess, &app));
        assert!(allows(&dev_partner, Action::AppAccess, &app));
        // A managing partner WITHOUT develop_apps must NOT reach the data plane — that
        // is the manage_apps vs develop_apps split.
        let manage_only = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &[Cap::ManageApps])],
            ..facts()
        };
        assert!(!allows(&manage_only, Action::AppAccess, &app));
        // A global owner reaches it too: both operator tiers reach every customer-app
        // surface. This used to be admin-only, so an owner not in `app_admins` could
        // PUBLISH an app but not VIEW one.
        let go = PrincipalFacts {
            is_global_owner: true,
            ..facts()
        };
        assert!(allows(&go, Action::AppAccess, &app));
        // A plain outsider is denied.
        assert!(!allows(&facts(), Action::AppAccess, &app));
    }

    #[test]
    fn restricted_app_drops_plain_org_membership_but_keeps_break_glass() {
        let app_id = Uuid::from_u128(950);
        let open = Resource::app_with_visibility(app_id, org(), false);
        let restricted = Resource::app_with_visibility(app_id, org(), true);

        // A plain org member: reaches an open app, DENIED a restricted one. This
        // is the subtraction the whole feature exists for.
        let member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(allows(&member, Action::AppAccess, &open));
        assert!(!allows(&member, Action::AppAccess, &restricted));

        // The same member WITH an app_members row reaches it (either role).
        let app_member = PrincipalFacts {
            member_orgs: vec![org()],
            app_memberships: vec![app_id],
            ..facts()
        };
        assert!(allows(&app_member, Action::AppAccess, &restricted));

        // A membership in a DIFFERENT app doesn't help.
        let elsewhere = PrincipalFacts {
            member_orgs: vec![org()],
            app_memberships: vec![Uuid::from_u128(951)],
            ..facts()
        };
        assert!(!allows(&elsewhere, Action::AppAccess, &restricted));

        // Break-glass: an org officer (owner OR admin) and Oxy staff still reach a
        // restricted app, so an org can't lock itself (or support) out of its app.
        let owner = PrincipalFacts {
            owned_orgs: vec![org()],
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        let org_admin = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        let staff = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        assert!(allows(&owner, Action::AppAccess, &restricted));
        assert!(allows(&org_admin, Action::AppAccess, &open));
        assert!(allows(&org_admin, Action::AppAccess, &restricted));
        assert!(allows(&staff, Action::AppAccess, &restricted));

        // A develop_apps partner keeps its data-plane reach.
        let dev_partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &[Cap::DevelopApps])],
            ..facts()
        };
        assert!(allows(&dev_partner, Action::AppAccess, &restricted));

        // An outsider is denied either way.
        let outsider = PrincipalFacts {
            member_orgs: vec![other_org()],
            ..facts()
        };
        assert!(!allows(&outsider, Action::AppAccess, &open));
        assert!(!allows(&outsider, Action::AppAccess, &restricted));
    }

    #[test]
    fn app_admin_is_an_org_officer_the_app_admin_role_or_staff() {
        let app_id = Uuid::from_u128(960);
        let app = Resource::app_with_visibility(app_id, org(), false);

        // The feature's reason to exist: an app admin who is only a plain org
        // member — admin rights extended DOWNWARD to a non-officer.
        let app_admin = PrincipalFacts {
            member_orgs: vec![org()],
            app_memberships: vec![app_id],
            app_admin_memberships: vec![app_id],
            ..facts()
        };
        assert!(allows(&app_admin, Action::AppAdmin, &app));

        // A non-admin app member reaches the app but NOT its privileged surface.
        let app_member = PrincipalFacts {
            member_orgs: vec![org()],
            app_memberships: vec![app_id],
            ..facts()
        };
        assert!(allows(&app_member, Action::AppAccess, &app));
        assert!(!allows(&app_member, Action::AppAdmin, &app));

        // Any org officer (owner OR admin) administers every app in the org, and
        // staff are break-glass.
        let owner = PrincipalFacts {
            owned_orgs: vec![org()],
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        let org_admin = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        let staff = PrincipalFacts {
            is_global_owner: true,
            ..facts()
        };
        assert!(allows(&owner, Action::AppAdmin, &app));
        assert!(allows(&org_admin, Action::AppAdmin, &app));
        assert!(allows(&staff, Action::AppAdmin, &app));

        // But a plain org MEMBER (no app-admin row) does not administer it — the
        // line is at officer, not member.
        let plain_member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(!allows(&plain_member, Action::AppAdmin, &app));

        // Admin of a DIFFERENT app grants nothing here.
        let other_app_admin = PrincipalFacts {
            member_orgs: vec![org()],
            app_admin_memberships: vec![Uuid::from_u128(961)],
            ..facts()
        };
        assert!(!allows(&other_app_admin, Action::AppAdmin, &app));

        // A develop_apps partner builds the app but does not administer it.
        let dev_partner = PrincipalFacts {
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        assert!(!allows(&dev_partner, Action::AppAdmin, &app));

        // A plain member and an outsider hold nothing.
        assert!(!allows(&facts(), Action::AppAdmin, &app));
    }

    #[test]
    fn app_membership_facts_do_not_leak_into_other_rings() {
        // Pins the blast radius: holding an app-admin row must not confer org or
        // workspace authority — the failure mode if a future ring reads these sets.
        let app_id = Uuid::from_u128(970);
        let f = PrincipalFacts {
            member_orgs: vec![org()],
            app_memberships: vec![app_id],
            app_admin_memberships: vec![app_id],
            ..facts()
        };
        assert!(!allows(&f, Action::MemberSetRole, &Resource::org(org())));
        assert!(!allows(&f, Action::OrgBilling, &Resource::org(org())));
        assert!(!allows(&f, Action::OrgOwnerManage, &Resource::org(org())));
        assert!(!allows(
            &f,
            Action::WorkspaceManage,
            &Resource::workspace(Uuid::from_u128(971), org())
        ));
        assert!(!allows(&f, Action::PlatformOps, &Resource::platform()));
    }

    #[test]
    fn partner_ceiling_is_per_capability_and_scoped_to_the_acting_partner() {
        let a = partner_org();
        let b = Uuid::from_u128(901);
        // Acting as partner A, on A's client.
        let as_a = Resource::partner_client(org(), a);

        // A grants manage_apps (lifecycle) but NOT develop_apps (the data plane). This
        // split is the one a coarse model collapses — and collapsing it hands a
        // lifecycle-only partner another tenant's app data.
        let lifecycle_only = PrincipalFacts {
            partners: vec![standing(a, org(), &[Cap::ManageApps])],
            ..facts()
        };
        assert!(allows(&lifecycle_only, Action::PartnerManageApps, &as_a));
        assert!(!allows(&lifecycle_only, Action::PartnerDevelopApps, &as_a));
        assert!(!allows(
            &lifecycle_only,
            Action::AppAccess,
            &Resource::org(org())
        ));
        // Holding one capability grants no other.
        assert!(!allows(
            &lifecycle_only,
            Action::PartnerManageBilling,
            &as_a
        ));
        assert!(!allows(&lifecycle_only, Action::PartnerViewAudit, &as_a));

        // THE SCOPE PROPERTY. Operating BOTH A and B, where only B grants view_audit:
        // acting as A must not borrow B's ceiling, even over a client both manage.
        let two = PrincipalFacts {
            partners: vec![
                standing(a, org(), &[Cap::ManageApps]),
                standing(b, org(), &[Cap::ViewAudit]),
            ],
            ..facts()
        };
        assert!(allows(
            &two,
            Action::PartnerViewAudit,
            &Resource::partner_client(org(), b)
        ));
        assert!(
            !allows(&two, Action::PartnerViewAudit, &as_a),
            "acting as A must not borrow B's ceiling — the scope a flattened model drops"
        );

        // A capability over one client says nothing about another.
        let elsewhere = PrincipalFacts {
            partners: vec![standing(a, org(), &[Cap::ViewAudit])],
            ..facts()
        };
        assert!(!allows(
            &elsewhere,
            Action::PartnerViewAudit,
            &Resource::partner_client(other_org(), a)
        ));

        // A partner decision must NAME the partner being acted as; an unscoped resource
        // can never authorize one.
        assert!(!allows(
            &two,
            Action::PartnerViewAudit,
            &Resource::org(org())
        ));

        // An empty ceiling holds nothing; a plain member holds no partner capability.
        let no_ceiling = PrincipalFacts {
            partners: vec![standing(a, org(), &[])],
            ..facts()
        };
        for cap in Cap::ALL {
            let action = match cap {
                Cap::ManageMembers => Action::PartnerManageMembers,
                Cap::ManageApps => Action::PartnerManageApps,
                Cap::DevelopApps => Action::PartnerDevelopApps,
                Cap::ViewAudit => Action::PartnerViewAudit,
                Cap::ManageBilling => Action::PartnerManageBilling,
                Cap::ManageSecrets => Action::PartnerManageSecrets,
                Cap::CreateOrgs => Action::PartnerCreateOrgs,
                Cap::ManageOrgSettings => Action::PartnerManageOrgSettings,
            };
            assert!(
                !allows(&no_ceiling, action, &as_a),
                "{cap:?} leaked with no ceiling"
            );
        }
        let member = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(!allows(&member, Action::PartnerManageApps, &as_a));
    }

    #[test]
    fn an_assumed_standing_is_the_ceiling_and_no_more() {
        // Staff looking through a partner's eyes get a standing exactly like a real
        // operator's — the partner's ceiling, never more. This is what the model used
        // to be blind to: the console's scope knew about the override and the facts
        // didn't, so the rings would have denied staff their own console.
        let a = partner_org();
        let assumed = PrincipalFacts {
            partners: vec![standing(a, org(), &[Cap::ManageApps, Cap::ViewAudit])],
            // Staff, but that must not add anything on the partner rings.
            is_global_owner: true,
            ..facts()
        };
        let as_a = Resource::partner_client(org(), a);
        assert!(allows(&assumed, Action::PartnerManageApps, &as_a));
        assert!(allows(&assumed, Action::PartnerViewAudit, &as_a));
        // Off in the ceiling stays off — on a PARTNER ring, staff see the same walls the
        // customer does. Assuming a partner does not widen what that partner may do.
        assert!(!allows(&assumed, Action::PartnerManageBilling, &as_a));
        assert!(!allows(&assumed, Action::PartnerDevelopApps, &as_a));
        // AppAccess is deliberately NOT asserted here: it is not a partner ring. It
        // grants any Oxy operator directly, so this principal reaches it on their staff
        // standing regardless of the ceiling — a different question with a different
        // answer, and conflating the two is how a ring gets modeled wrong.
    }

    #[test]
    fn workspace_rename_admits_org_admin_or_the_creator_only() {
        let ws = Uuid::from_u128(800);
        // A plain member who CREATED this workspace.
        let creator = PrincipalFacts {
            member_orgs: vec![org()],
            ..facts()
        };
        let by_creator = Resource::workspace_with_creator(ws, org(), Some(user()));
        assert!(allows(&creator, Action::WorkspaceRename, &by_creator));

        // The same plain member on a workspace someone ELSE created: denied.
        let by_other = Resource::workspace_with_creator(ws, org(), Some(Uuid::from_u128(101)));
        assert!(!allows(&creator, Action::WorkspaceRename, &by_other));
        // ...and when the creator is unknown.
        let by_nobody = Resource::workspace_with_creator(ws, org(), None);
        assert!(!allows(&creator, Action::WorkspaceRename, &by_nobody));

        // An org admin renames any workspace, creator or not.
        let admin = PrincipalFacts {
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            ..facts()
        };
        assert!(allows(&admin, Action::WorkspaceRename, &by_other));

        // The creator claim is confined to rename — it must not imply a privileged
        // workspace action.
        assert!(!allows(&creator, Action::WorkspaceManage, &by_creator));
        // ...nor reach an outsider's workspace.
        let outsider = PrincipalFacts {
            member_orgs: vec![other_org()],
            ..facts()
        };
        assert!(!allows(&outsider, Action::WorkspaceRename, &by_other));
    }

    #[test]
    fn platform_tier_reads_only_global_flags_and_is_unreachable_from_a_tenant_role() {
        let p = Resource::platform();
        let ga = PrincipalFacts {
            is_global_admin: true,
            ..facts()
        };
        let go = PrincipalFacts {
            is_global_owner: true,
            ..facts()
        };
        // platform_ops: either global.
        assert!(allows(&ga, Action::PlatformOps, &p));
        assert!(allows(&go, Action::PlatformOps, &p));
        // There is no admin-only action: both tiers reach every operator surface.
        // platform_owner_only: OWNER only.
        assert!(allows(&go, Action::PlatformOwnerOnly, &p));
        assert!(!allows(&ga, Action::PlatformOwnerOnly, &p));

        // The isolation property: no tenant standing — however senior — reaches the
        // platform. Platform is parentless, so no org set can resolve onto it.
        let org_owner = PrincipalFacts {
            owned_orgs: vec![org()],
            admin_orgs: vec![org()],
            member_orgs: vec![org()],
            partners: vec![standing(partner_org(), org(), &Cap::ALL)],
            ..facts()
        };
        assert!(!allows(&org_owner, Action::PlatformOps, &p));
        assert!(!allows(&org_owner, Action::PlatformOwnerOnly, &p));
        // And the converse: a platform action can't be smuggled onto a tenant org.
        assert!(!allows(
            &go,
            Action::PlatformOwnerOnly,
            &Resource::org(org())
        ));
    }
}
