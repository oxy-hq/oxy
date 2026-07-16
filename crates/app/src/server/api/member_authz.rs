//! Pure org-membership authority decisions — the RBAC rules that govern who may
//! change or remove whom across the org-admin and partner-admin tiers. Extracted
//! from the `organizations` and `partner_console` handlers so the conflict
//! invariants are unit-testable without a DB. The transactional **last-owner
//! count** stays in the handlers (it needs row locks); everything here is pure.
//!
//! Full model + citations: `internal-docs/2026-07-16-partner-platform-design.md`.
//!
//! The load-bearing invariants encoded here:
//!   1. Owner is sovereign & protected — only an Owner may touch an Owner/Admin.
//!   2. No escalation — only an Owner may grant Owner.
//!   3. Partner guardrail — a partner admin never assigns or touches an Owner,
//!      so it can neither seize an org nor break the last-owner invariant.

use axum::http::StatusCode;
use entity::org_members::OrgRole;

// --- Org-admin tier -------------------------------------------------------

/// Pre-load checks for an org-admin role change: an Owner can't change their own
/// role inline (transfer ownership / leave instead), and only an Owner may grant
/// the Owner role (no escalation).
pub fn authorize_role_change_intent(
    actor_role: &OrgRole,
    is_self: bool,
    new_role: &OrgRole,
) -> Result<(), StatusCode> {
    if is_self && *actor_role == OrgRole::Owner {
        return Err(StatusCode::BAD_REQUEST);
    }
    if *new_role == OrgRole::Owner && *actor_role != OrgRole::Owner {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Post-load check shared by role-change and removal: only an Owner may modify a
/// target that is itself an Owner or Admin. Admins may act on Members only.
pub fn authorize_target_modification(
    actor_role: &OrgRole,
    target_role: &OrgRole,
) -> Result<(), StatusCode> {
    if matches!(target_role, OrgRole::Owner | OrgRole::Admin) && *actor_role != OrgRole::Owner {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Pre-load check for an org-admin removal: an Owner can't remove themselves
/// inline (transfer ownership first).
pub fn authorize_removal_intent(actor_role: &OrgRole, is_self: bool) -> Result<(), StatusCode> {
    if is_self && *actor_role == OrgRole::Owner {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

// --- Partner-admin tier ---------------------------------------------------

/// The partner guardrail on *assigning* a role: a partner admin may grant
/// Member or Admin, never Owner. Keeps a partner structurally incapable of
/// minting an owner (org seizure / last-owner break).
pub fn partner_may_assign(role: &OrgRole) -> bool {
    *role != OrgRole::Owner
}

/// The partner guardrail on *modifying an existing* member: a partner admin may
/// never change the role of, or remove, an Owner.
pub fn partner_may_modify(target_role: &OrgRole) -> bool {
    *target_role != OrgRole::Owner
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Invariant 1: Owner is sovereign & protected (org tier) ----------

    #[test]
    fn admin_cannot_modify_admin_or_owner() {
        // An Org Admin acting on an Admin or Owner target is forbidden.
        assert_eq!(
            authorize_target_modification(&OrgRole::Admin, &OrgRole::Admin),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            authorize_target_modification(&OrgRole::Admin, &OrgRole::Owner),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn admin_may_modify_member() {
        assert_eq!(
            authorize_target_modification(&OrgRole::Admin, &OrgRole::Member),
            Ok(())
        );
    }

    #[test]
    fn owner_may_modify_anyone() {
        for target in [OrgRole::Owner, OrgRole::Admin, OrgRole::Member] {
            assert_eq!(
                authorize_target_modification(&OrgRole::Owner, &target),
                Ok(())
            );
        }
    }

    // --- Invariant 2: no escalation to Owner (org tier) ------------------

    #[test]
    fn only_owner_grants_owner() {
        assert_eq!(
            authorize_role_change_intent(&OrgRole::Admin, false, &OrgRole::Owner),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            authorize_role_change_intent(&OrgRole::Owner, false, &OrgRole::Owner),
            Ok(())
        );
    }

    #[test]
    fn admin_may_set_member_and_admin() {
        assert_eq!(
            authorize_role_change_intent(&OrgRole::Admin, false, &OrgRole::Member),
            Ok(())
        );
        assert_eq!(
            authorize_role_change_intent(&OrgRole::Admin, false, &OrgRole::Admin),
            Ok(())
        );
    }

    #[test]
    fn owner_cannot_self_change_or_self_remove() {
        assert_eq!(
            authorize_role_change_intent(&OrgRole::Owner, true, &OrgRole::Admin),
            Err(StatusCode::BAD_REQUEST)
        );
        assert_eq!(
            authorize_removal_intent(&OrgRole::Owner, true),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn non_owner_self_actions_are_not_blocked_here() {
        // A self-acting Admin isn't blocked by the intent guard (the target
        // guard still applies downstream).
        assert_eq!(
            authorize_role_change_intent(&OrgRole::Admin, true, &OrgRole::Member),
            Ok(())
        );
        assert_eq!(authorize_removal_intent(&OrgRole::Admin, true), Ok(()));
    }

    // --- Removal authority (org tier) ------------------------------------

    #[test]
    fn admin_cannot_remove_admin_or_owner_but_can_remove_member() {
        assert_eq!(
            authorize_target_modification(&OrgRole::Admin, &OrgRole::Owner),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            authorize_target_modification(&OrgRole::Admin, &OrgRole::Admin),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            authorize_target_modification(&OrgRole::Admin, &OrgRole::Member),
            Ok(())
        );
    }

    // --- Invariant 3: partner guardrail (partner tier) -------------------

    #[test]
    fn partner_may_not_assign_owner() {
        assert!(!partner_may_assign(&OrgRole::Owner));
        assert!(partner_may_assign(&OrgRole::Admin));
        assert!(partner_may_assign(&OrgRole::Member));
    }

    #[test]
    fn partner_may_not_modify_owner() {
        // The conflict crux: a partner admin can never demote/remove an org
        // Owner, so it cannot seize an org even with manage_members.
        assert!(!partner_may_modify(&OrgRole::Owner));
        assert!(partner_may_modify(&OrgRole::Admin));
        assert!(partner_may_modify(&OrgRole::Member));
    }
}
