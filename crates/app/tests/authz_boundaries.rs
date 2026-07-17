//! Boundary test: authorization must not scatter again.
//!
//! ## Why a source-scanning test
//!
//! Authorization did not scatter because anyone decided to duplicate it. It scattered
//! because deciding access inline is *always the shortest path* — one `matches!` beside
//! the handler beats reaching for a guard. Nothing objected, so ~170 call sites each
//! re-decided access, and the copies drifted from their siblings.
//!
//! Unifying them once fixes today. It does not stop the next handler from taking the same
//! shortcut tomorrow — and a reviewer who does not already know the model will not catch
//! it. So the objection has to be mechanical: this test fails the build when a file
//! outside the allowlist decides authorization by hand.
//!
//! It found the first one it ran against. The manual sweeps that built this model were
//! all scoped to `server/` and never looked at `integrations/`, where the Slack OAuth
//! handlers had been hand-rolling the org-admin ring the whole time.
//!
//! ## The allowlist is the backlog, not an exemption list
//!
//! Every entry states why that file is still allowed to say this. Most are **legacy
//! terms**: the `existing_allow` half of `enforce(…, existing_allow)`, which is exactly
//! the hand-rolled check the model is being differenced against. They are supposed to
//! exist right now and are supposed to disappear when a ring stops needing its legacy
//! term — so shrinking this list is the migration, and the list going up is the signal.
//!
//! Adding an entry is deliberately annoying: you must edit this file and write down why.
//! That is the point. It converts an invisible shortcut into a reviewable act.
//!
//! ## Precision over reach
//!
//! Only shapes that are *unambiguously* a hand-rolled decision are banned. `== OrgRole::Owner`
//! is deliberately **not** banned: it matches `role: Set(OrgRole::Owner)` when creating a
//! membership and the last-owner invariant in `organizations.rs`, neither of which is a
//! ring. A test that cries wolf gets deleted, and then it protects nothing.

use std::fs;
use std::path::{Path, PathBuf};

/// A source shape that means "this file is deciding authorization by hand".
struct Banned {
    /// Literal substring. Kept literal (not a regex) so what it matches is obvious.
    needle: &'static str,
    /// The door this caller should be taking instead.
    door: &'static str,
}

const BANNED: &[Banned] = &[
    Banned {
        needle: "OrgRole::Owner | OrgRole::Admin",
        door: "this is the OrgAdmin ring. Take the `role_guards::OrgAdmin` extractor, or \
               call `authz::enforce_guard(.., Action::MemberSetRole, ..)`. Do not restate it.",
    },
    Banned {
        needle: "is_oxy_owner(",
        door: "read platform standing through `server::authz::globals` — \
               `platform_standing()` for a flag to display, or `allows(.., Action::Platform*, ..)` \
               for a decision.",
    },
    Banned {
        needle: "is_app_admin_email(",
        door: "read platform standing through `server::authz::globals` — see `is_oxy_owner`.",
    },
    Banned {
        needle: "is_oxy_app_admin(",
        door: "read platform standing through `server::authz::globals` — see `is_oxy_owner`.",
    },
];

/// Files permitted to contain a banned shape, and why. Paths are relative to `src/`.
///
/// **Shrinking this list is the migration.** Before adding to it, check you are not
/// simply taking the shortcut this test exists to object to.
const ALLOWED: &[(&str, &str)] = &[
    // ---- The model and its fact loader. Someone has to say it once; this is where. ----
    (
        "server/authz/",
        "the model, the loader, and their differential tests",
    ),
    // ---- Legacy terms: the `existing_allow` half of the conjunction. ----
    // These are the shipped checks the model is differenced against. They are load-bearing
    // until a ring is enforced on its own, and they are what should disappear first.
    (
        "server/api/middlewares/role_guards.rs",
        "legacy terms — the six guards' `existing_allow` half",
    ),
    (
        "server/api/middlewares/oxy_owner_guard.rs",
        "defines `is_oxy_owner` (the OXY_OWNER env read `globals` wraps)",
    ),
    (
        "server/api/middlewares/oxy_app_admin_guard.rs",
        "legacy term + the `is_oxy_app_admin` wrapper",
    ),
    (
        "server/api/middlewares/oxy_owner_or_app_admin_guard.rs",
        "legacy term — the `is_staff` shape",
    ),
    (
        "server/api/customer_apps_auth.rs",
        "defines `is_app_admin_email` (the `app_admins` read `globals` wraps)",
    ),
    (
        "server/api/partner_console/people.rs",
        "legacy term for the partner-console ring",
    ),
    // ---- Deliberate non-merges. Different question, not a duplicated answer. ----
    (
        "server/api/member_authz.rs",
        "the escalation guardrail, and it reads the *target's* role, not the principal's: \
         `matches!(target_role, Owner | Admin) && actor_role != Owner` says 'making someone an \
         officer requires an Owner'. That is a role-transition invariant, not the OrgAdmin ring \
         — the needle matches the set {Owner, Admin}, not the question being asked of it",
    ),
    (
        "server/api/customer_apps_publish_authz.rs",
        "a named decision module of shape actor x state -> reason, not principal-action-resource; \
         it must explain *why* it denied, which a ring cannot express",
    ),
    (
        "server/api/workspace_members.rs",
        "role-*transition* invariants (actor x target x new role). `target.role >= caller.role` is \
         NOT `authorize_target_modification`: for Owner-on-Owner the former denies and the latter \
         allows, so merging them would silently change behaviour",
    ),
    // ---- Surfaced, awaiting a decision. See the divergence note below. ----
    (
        "integrations/slack/oauth/callback.rs",
        "KNOWN DIVERGENCE, not an exemption: re-verifies the actor at OAuth-callback time from a \
         raw `org_members` row. Slack redirects the browser back with no Authorization header, so \
         there is no request auth context and no OrgContext — and the synthetic-Owner override \
         lives in `org_context`, not in facts. So this site cannot reproduce the ring without \
         duplicating assume-session resolution. Consequence: staff assuming an org pass \
         `start_install` (which honours the override) and then get 403 'no longer a member' here. \
         Fixing it is a security call about an OAuth flow, not a cleanup",
    ),
];

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn authorization_does_not_scatter_outside_the_allowlist() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&src, &mut files);
    assert!(
        files.len() > 100,
        "expected to scan the whole oxy-app source tree, found only {} files — the walk is \
         broken, and a boundary test that silently scans nothing is worse than no test",
        files.len()
    );

    let mut violations = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED.iter().any(|(allowed, _)| rel.starts_with(allowed)) {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for banned in BANNED {
            for (i, line) in text.lines().enumerate() {
                if line.contains(banned.needle) {
                    violations.push(format!(
                        "  src/{rel}:{}\n    found: {}\n    instead: {}",
                        i + 1,
                        line.trim(),
                        banned.door
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "\n{} site(s) are deciding authorization by hand:\n\n{}\n\n\
         Authorization is stated once, in `oxy_authz::allows`. A check here is a copy, and \
         copies drift from the original — that is how the ~170-site scatter this model replaced \
         got built, one reasonable shortcut at a time.\n\n\
         If this really is a new legacy term or a genuinely different question, add it to \
         ALLOWED in tests/authz_boundaries.rs with the reason. Writing the reason down is the \
         cost of admission.\n",
        violations.len(),
        violations.join("\n\n")
    );
}
