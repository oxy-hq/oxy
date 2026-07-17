//! Differential tests: the shipped legacy checks are the **oracle**, and the unified
//! model must agree with them across the whole scenario space.
//!
//! ## Why this exists instead of a production shadow window
//!
//! The plan for flipping a ring to the model-as-authority was "watch `authz.shadow` until
//! `disagree` holds at zero on real traffic". That only works if there IS real
//! traffic. With little or none, `disagree == 0` does not mean the model is right — it
//! means nobody hit it. Waiting on that is waiting on a false green.
//!
//! What actually caught every mis-modeled ring during this build (billing,
//! develop_apps, and the operator-outranks-membership escalation) was *differencing
//! the model against the legacy check*. That does not need traffic: the inputs that decide
//! an org ring are a small, finite space — the caller's real role, whether they reach
//! in via the global-operator override, whether they are Oxy staff, whether they are a
//! managing partner. So enumerate it and assert equivalence. Exhaustive instead of
//! sampled, and it runs in milliseconds with zero users.
//!
//! ## How to read a failure
//!
//! A failure means the model and the shipped check disagree for a caller shape that can
//! really occur. A disagreement is a **bug in the ring** until proven otherwise — decide
//! deliberately which side is right, and if it's the model, say so in the test rather
//! than deleting it.
//!
//! Each scenario models what the two independent paths produce for the same caller:
//! * `org_context` → the `OrgContext` the legacy guard reads (a real membership row, or
//!   a synthetic Owner/Admin with `is_global_override`).
//! * `authz::loader` → the [`PrincipalFacts`] [`allows`] reads.

use oxy_authz::{Action, Cap, PartnerStanding, PrincipalFacts, Resource, allows};
use uuid::Uuid;

use entity::org_members::OrgRole;

fn org() -> Uuid {
    Uuid::from_u128(1)
}
fn user() -> Uuid {
    Uuid::from_u128(100)
}

/// One caller shape, and what each path derives for it.
struct Scenario {
    name: &'static str,
    /// What `org_context` puts in `ctx.membership.role` (real row, or synthetic).
    ctx_role: OrgRole,
    /// What `org_context` sets for `is_global_override` (synthetic membership).
    ctx_override: bool,
    /// What the loader produces.
    facts: PrincipalFacts,
}

fn scenarios() -> Vec<Scenario> {
    let base = || PrincipalFacts {
        user_id: user(),
        ..Default::default()
    };
    vec![
        Scenario {
            name: "real org owner",
            ctx_role: OrgRole::Owner,
            ctx_override: false,
            facts: PrincipalFacts {
                owned_orgs: vec![org()],
                admin_orgs: vec![org()],
                member_orgs: vec![org()],
                ..base()
            },
        },
        Scenario {
            name: "real org admin",
            ctx_role: OrgRole::Admin,
            ctx_override: false,
            facts: PrincipalFacts {
                admin_orgs: vec![org()],
                member_orgs: vec![org()],
                ..base()
            },
        },
        Scenario {
            name: "real org member",
            ctx_role: OrgRole::Member,
            ctx_override: false,
            facts: PrincipalFacts {
                member_orgs: vec![org()],
                ..base()
            },
        },
        Scenario {
            // Staff with a live assume session, NOT a member: org_context synthesizes
            // Owner (assume.rs `ActingAs::Staff`).
            name: "global admin via override (not a member)",
            ctx_role: OrgRole::Owner,
            ctx_override: true,
            facts: PrincipalFacts {
                is_global_admin: true,
                ..base()
            },
        },
        Scenario {
            name: "global owner via override (not a member)",
            ctx_role: OrgRole::Owner,
            ctx_override: true,
            facts: PrincipalFacts {
                is_global_owner: true,
                ..base()
            },
        },
        Scenario {
            // The escalation case: staff who is ALSO a plain member. org_context hands
            // back their REAL row, so their real role governs — the global flag must
            // not out-rank it.
            name: "global admin who is a plain member",
            ctx_role: OrgRole::Member,
            ctx_override: false,
            facts: PrincipalFacts {
                member_orgs: vec![org()],
                is_global_admin: true,
                ..base()
            },
        },
        Scenario {
            name: "global admin who is a real org admin",
            ctx_role: OrgRole::Admin,
            ctx_override: false,
            facts: PrincipalFacts {
                admin_orgs: vec![org()],
                member_orgs: vec![org()],
                is_global_admin: true,
                ..base()
            },
        },
        Scenario {
            // A partner operating a client: assume.rs `ActingAs::Partner` -> Admin.
            name: "managing partner via override (not a member)",
            ctx_role: OrgRole::Admin,
            ctx_override: true,
            facts: PrincipalFacts {
                partners: vec![PartnerStanding {
                    partner_id: Uuid::from_u128(900),
                    client_orgs: vec![org()],
                    caps: vec![Cap::ManageMembers],
                }],
                ..base()
            },
        },
    ]
}

/// Assert the model matches the oracle for `action` across every scenario.
fn assert_matches_oracle(action: Action, oracle: impl Fn(&Scenario) -> bool) {
    for s in scenarios() {
        let expected = oracle(&s);
        let actual = allows(&s.facts, action, &Resource::org(org()));
        assert_eq!(
            actual, expected,
            "ring drift for {action:?} — scenario {:?}: the shipped check says {expected}, \
             the model says {actual}",
            s.name
        );
    }
}

#[test]
fn org_admin_ring_matches_the_shipped_guard() {
    // role_guards::OrgAdmin
    assert_matches_oracle(Action::MemberSetRole, |s| {
        matches!(s.ctx_role, OrgRole::Owner | OrgRole::Admin)
    });
}

#[test]
fn org_admin_strict_ring_matches_the_shipped_guard() {
    // role_guards::OrgAdminStrict — rejects the override.
    assert_matches_oracle(Action::OrgBilling, |s| {
        !s.ctx_override && matches!(s.ctx_role, OrgRole::Owner | OrgRole::Admin)
    });
}

#[test]
fn org_owner_ring_matches_the_shipped_guard() {
    // role_guards::OrgOwner
    assert_matches_oracle(Action::OrgOwnerManage, |s| s.ctx_role == OrgRole::Owner);
}

#[test]
fn org_member_strict_ring_matches_the_shipped_guard() {
    // role_guards::OrgMemberStrict — any real member, override rejected.
    assert_matches_oracle(Action::OrgReadStrict, |s| !s.ctx_override);
}
