//! Cedar-backed authorization for the partner tier (ADR
//! `internal-docs/2026-07-16-partner-platform-design.md`).
//!
//! The DECISION logic — "does this partner hold the capability, and (for
//! org-scoped actions) does it own the resource?" — is declarative Cedar policy
//! text evaluated by Cedar's formally-verified engine, in process (no external
//! service). ENFORCEMENT stays in the typed guards (`require_org_scope`,
//! `PartnerAdmin::require`), which call the `authorize_*` functions here and map
//! a Deny to the right HTTP status.
//!
//! **Ownership is modelled, not asserted** (review #4). Earlier this passed
//! `owns: bool` in as request *context* — which meant Rust had already decided
//! the only interesting question and Cedar merely re-checked `owns && has_cap`, a
//! tautology over `Entities::empty()`. Now the partner and the orgs it manages
//! are real Cedar **entities**: each managed `Org` is a child of the
//! `PartnerAdmin`, and the policy asks `resource in principal`. Cedar evaluates
//! the ownership relation itself against the entity graph; Rust only supplies
//! facts from the DB (the partner's capability set and its managed-org list).
//!
//! Rust still maps a Deny to 404-vs-403 for existence hiding — that's a
//! presentation choice, not the security decision.
//!
//! Fails closed: any parse/build/eval error denies.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use cedar_policy::{
    Authorizer, Context, Decision, Entities, Entity, EntityUid, PolicySet, Request,
    RestrictedExpression,
};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use super::partner_authz::{self, PartnerCapability, PartnerScope};

/// The complete partner authorization policy — ONE uniform Cedar `permit` per
/// capability, **generated from [`PartnerCapability::ALL`]** so the enum is the
/// single source of truth. Add a capability variant (+ its `as_str`) and its
/// policy rule, its `granted_names` entry, and the `Action` it authorizes all
/// follow automatically — there is no hand-maintained policy string to drift
/// from the enum. Every rule has the identical shape:
///
/// ```text
/// permit(principal, action == Action::"<cap>", resource)
/// when { resource in principal && principal.capabilities.contains("<cap>") };
/// ```
///
/// Both halves of the decision are Cedar's:
///   * `resource in principal` — the org is a child of this partner in the
///     entity graph (see [`entities`]). **This is the ownership check.**
///   * `principal.capabilities.contains("<cap>")` — the partner holds the cap.
///
/// If a capability ever needs a non-uniform rule (a different `when`), give it
/// an explicit branch here instead of the generated one — today all eight are
/// uniform, so generation keeps them honest.
fn policy_source() -> String {
    PartnerCapability::ALL
        .into_iter()
        .map(|c| {
            let cap = c.as_str();
            format!(
                "permit(principal, action == Action::\"{cap}\", resource)\n\
                 when {{ resource in principal && principal.capabilities.contains(\"{cap}\") }};"
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn policies() -> &'static PolicySet {
    static SET: OnceLock<PolicySet> = OnceLock::new();
    SET.get_or_init(|| {
        policy_source()
            .parse()
            .expect("generated partner Cedar policy is derived from a fixed enum and must parse")
    })
}

fn partner_uid(partner_id: Uuid) -> Option<EntityUid> {
    format!(r#"PartnerAdmin::"{partner_id}""#).parse().ok()
}

fn org_uid(org_id: Uuid) -> Option<EntityUid> {
    format!(r#"Org::"{org_id}""#).parse().ok()
}

/// The entity graph Cedar reasons over:
///   `PartnerAdmin::"<id>"` — the principal, carrying `capabilities` (a set of
///   strings) as an attribute.
///   `Org::"<id>"` — one per org the partner **actually manages**, each declared
///   with the partner as its PARENT.
///
/// Because managed orgs are children of the partner, `resource in principal`
/// evaluates to true exactly for the orgs in the partner's real subtree. That is
/// what turns ownership from a Rust boolean into a policy decision.
fn entities(scope: &PartnerScope, managed_orgs: &[Uuid]) -> Option<Entities> {
    let principal = partner_uid(scope.partner_id)?;

    let caps = RestrictedExpression::new_set(
        scope
            .capabilities
            .granted_names()
            .into_iter()
            .map(|c| RestrictedExpression::new_string(c.to_string())),
    );
    let mut attrs = HashMap::new();
    attrs.insert("capabilities".to_string(), caps);

    let mut all = vec![Entity::new(principal.clone(), attrs, HashSet::new()).ok()?];
    for org_id in managed_orgs {
        let mut parents = HashSet::new();
        parents.insert(principal.clone());
        all.push(Entity::new(org_uid(*org_id)?, HashMap::new(), parents).ok()?);
    }
    Entities::from_entities(all, None).ok()
}

/// Core evaluation against the entity graph. Fails closed on any
/// construction/eval error.
fn evaluate(
    entities: &Entities,
    principal: EntityUid,
    resource: EntityUid,
    cap: PartnerCapability,
) -> bool {
    let Ok(action) = format!(r#"Action::"{}""#, cap.as_str()).parse::<EntityUid>() else {
        tracing::error!("partner_policy: action uid parse failed");
        return false;
    };
    let request = match Request::new(principal, action, resource, Context::empty(), None) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("partner_policy: request build failed: {e}");
            return false;
        }
    };
    Authorizer::new()
        .is_authorized(&request, policies(), entities)
        .decision()
        == Decision::Allow
}

/// Authorize an org-scoped partner action. `managed_orgs` is the partner's real
/// subtree (from `partner_org_ids`); Cedar decides both whether `org_id` is in it
/// and whether the partner holds `cap`.
pub fn authorize_org(
    scope: &PartnerScope,
    org_id: Uuid,
    managed_orgs: &[Uuid],
    cap: PartnerCapability,
) -> bool {
    let (Some(principal), Some(resource), Some(ents)) = (
        partner_uid(scope.partner_id),
        org_uid(org_id),
        entities(scope, managed_orgs),
    ) else {
        tracing::error!("partner_policy: entity graph build failed");
        return false;
    };
    evaluate(&ents, principal, resource, cap)
}

/// Whether `user` may reach `org_id`'s custom-app data plane via partner
/// delegation. True iff the caller is an active admin of the partner that
/// manages the org AND that partner holds `manage_apps` (the Cedar decision).
/// Fails closed. Called by the custom-app gate so the console, admin preview,
/// and serve/proxy share one decision (design doc §4).
pub async fn partner_grants_app_access(
    db: &DatabaseConnection,
    user_id: Uuid,
    user_email: &str,
    org_id: Uuid,
) -> bool {
    let Some(partner_id) = partner_authz::partner_for_org(db, org_id).await else {
        return false;
    };
    // resolve_scope confirms the caller is an active admin of that partner; since
    // partner_for_org returned it, the partner manages the org → owns = true.
    let Some(scope) = partner_authz::resolve_scope(db, partner_id, user_id, user_email).await
    else {
        return false;
    };
    // The DATA PLANE requires develop_apps — manage_apps is lifecycle only. Cedar
    // still derives ownership from the partner's managed-org set (every operator
    // reaches all of them), so this only holds for orgs the partner actually
    // manages.
    let managed = scope.org_ids.clone();
    authorize_org(&scope, org_id, &managed, PartnerCapability::DevelopApps)
}

/// Authorize a capability-only partner action (no external resource) — e.g.
/// reading the partner's own subtree audit log. The resource IS the partner, and
/// Cedar's `in` is reflexive (`p in p`), so `resource in principal` holds
/// trivially and the capability is what decides.
pub fn authorize_capability(scope: &PartnerScope, cap: PartnerCapability) -> bool {
    let (Some(principal), Some(ents)) = (partner_uid(scope.partner_id), entities(scope, &[]))
    else {
        tracing::error!("partner_policy: entity graph build failed");
        return false;
    };
    let resource = principal.clone();
    evaluate(&ents, principal, resource, cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::middlewares::partner_authz::Capabilities;

    /// A scope built the way a request actually builds one: the operator's authority
    /// IS the ceiling. The managed-org set is passed explicitly to `authorize_org`,
    /// so `org_ids` is irrelevant here.
    fn scope(caps: Capabilities) -> PartnerScope {
        PartnerScope {
            partner_id: Uuid::new_v4(),
            slug: "acme".into(),
            capabilities: caps,
            org_ids: Vec::new(),
        }
    }

    #[test]
    fn policies_parse() {
        // Forces the OnceLock init; panics if the generated policy is malformed.
        let set = policies();
        // Exactly one permit rule per capability — generation stays 1:1 with the
        // enum, so a capability can never silently lack a policy (or vice versa).
        assert_eq!(set.policies().count(), PartnerCapability::ALL.len());
    }

    #[test]
    fn allows_when_capability_and_ownership() {
        let s = scope(Capabilities {
            manage_members: true,
            ..Default::default()
        });
        let org = Uuid::new_v4();
        // `org` is IN the partner's managed subtree → Cedar's `resource in
        // principal` holds.
        assert!(authorize_org(
            &s,
            org,
            &[org],
            PartnerCapability::ManageMembers
        ));
    }

    #[test]
    fn denies_without_ownership() {
        let s = scope(Capabilities {
            manage_members: true,
            ..Default::default()
        });
        let org = Uuid::new_v4();
        // Empty subtree → the org is not a child of the partner, so Cedar denies
        // even though the capability is held. Ownership is the POLICY's decision.
        assert!(!authorize_org(
            &s,
            org,
            &[],
            PartnerCapability::ManageMembers
        ));
    }

    /// An operator reaches every client the partner manages — the whole subtree.
    #[test]
    fn an_operator_reaches_the_whole_subtree() {
        let s = scope(Capabilities {
            manage_members: true,
            ..Default::default()
        });
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        // resolve_scope hands the full subtree through when there are no rows.
        assert!(authorize_org(
            &s,
            a,
            &[a, b],
            PartnerCapability::ManageMembers
        ));
        assert!(authorize_org(
            &s,
            b,
            &[a, b],
            PartnerCapability::ManageMembers
        ));
    }

    /// Ownership is per-org, not blanket: managing org A does not grant org B.
    #[test]
    fn ownership_does_not_leak_across_orgs() {
        let s = scope(Capabilities {
            manage_members: true,
            ..Default::default()
        });
        let (managed, other) = (Uuid::new_v4(), Uuid::new_v4());
        assert!(authorize_org(
            &s,
            managed,
            &[managed],
            PartnerCapability::ManageMembers
        ));
        assert!(!authorize_org(
            &s,
            other,
            &[managed],
            PartnerCapability::ManageMembers
        ));
    }

    /// Review #2: publishing an app must NOT imply reading the org's data.
    /// A partner with `manage_apps` but not `develop_apps` can toggle app
    /// visibility and is denied the data plane (/query, /semantic-query, agents).
    #[test]
    fn manage_apps_does_not_grant_the_data_plane() {
        let s = scope(Capabilities {
            manage_apps: true,
            develop_apps: false,
            ..Default::default()
        });
        let org = Uuid::new_v4();
        // Lifecycle: allowed.
        assert!(authorize_org(
            &s,
            org,
            &[org],
            PartnerCapability::ManageApps
        ));
        // Data plane: DENIED — this is the capability split.
        assert!(!authorize_org(
            &s,
            org,
            &[org],
            PartnerCapability::DevelopApps
        ));
    }

    /// The converse: `develop_apps` is what the data plane keys on.
    #[test]
    fn develop_apps_grants_the_data_plane_only_when_owned() {
        let s = scope(Capabilities {
            develop_apps: true,
            ..Default::default()
        });
        let org = Uuid::new_v4();
        assert!(authorize_org(
            &s,
            org,
            &[org],
            PartnerCapability::DevelopApps
        ));
        // Still scoped: an org this partner does not manage is denied.
        assert!(!authorize_org(&s, org, &[], PartnerCapability::DevelopApps));
        // And it does not bleed into other capabilities.
        assert!(!authorize_org(
            &s,
            org,
            &[org],
            PartnerCapability::ManageApps
        ));
    }

    #[test]
    fn denies_without_capability() {
        let s = scope(Capabilities {
            manage_members: true, // has members, not apps
            ..Default::default()
        });
        let org = Uuid::new_v4();
        assert!(!authorize_org(
            &s,
            org,
            &[org],
            PartnerCapability::ManageApps
        ));
    }

    #[test]
    fn capability_only_check() {
        let s = scope(Capabilities {
            view_audit: true,
            ..Default::default()
        });
        assert!(authorize_capability(&s, PartnerCapability::ViewAudit));
        assert!(!authorize_capability(&s, PartnerCapability::ManageBilling));
    }

    /// Fail-closed: a partner with NO capabilities is denied every action even
    /// on an org it genuinely manages (ownership alone grants nothing).
    #[test]
    fn default_capabilities_deny_everything() {
        let s = scope(Capabilities::default());
        let org = Uuid::new_v4();
        for cap in [
            PartnerCapability::ManageMembers,
            PartnerCapability::ManageApps,
            PartnerCapability::DevelopApps,
            PartnerCapability::ViewAudit,
            PartnerCapability::ManageBilling,
            PartnerCapability::ManageSecrets,
        ] {
            assert!(!authorize_org(&s, org, &[org], cap));
        }
    }

    // ── the model's load-bearing claims, through the real Cedar path ─────────

    /// The ceiling is the whole capability story. An operator gets exactly what the
    /// ceiling grants and no more — publishing without the data plane, in this case.
    #[test]
    fn an_operator_gets_exactly_the_ceiling() {
        // Oxy granted apps + audit only.
        let narrow = Capabilities {
            manage_apps: true,
            view_audit: true,
            ..Default::default()
        };
        let op = scope(narrow);
        let org = Uuid::new_v4();

        assert!(authorize_org(
            &op,
            org,
            &[org],
            PartnerCapability::ManageApps
        ));
        // Everything the ceiling withholds is denied — no role can add it.
        for denied in [
            PartnerCapability::ManageMembers,
            PartnerCapability::DevelopApps,
            PartnerCapability::CreateOrgs,
            PartnerCapability::ManageBilling,
            PartnerCapability::ManageSecrets,
            PartnerCapability::ManageOrgSettings,
        ] {
            assert!(
                !authorize_org(&op, org, &[org], denied),
                "{denied:?} escaped the ceiling"
            );
        }
    }

    /// A member of the partner org with NO partner access never produces a scope at
    /// all — but if one were ever forged, an empty capability set must still deny.
    #[test]
    fn an_empty_capability_set_denies_everything() {
        let nobody = scope(Capabilities::default());
        let org = Uuid::new_v4();
        for cap in PartnerCapability::ALL {
            assert!(!authorize_org(&nobody, org, &[org], cap));
        }
    }
}
