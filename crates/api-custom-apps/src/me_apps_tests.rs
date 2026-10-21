//! `GET /api/me/apps`: which apps the ring keeps, and how a card is mapped.

use super::*;
use crate::custom_apps_agent_fixture as fixture;
use crate::custom_apps_manifest::OxyAppManifest;
use crate::workspace_custom_apps::{CustomAppSummary, icon_art_urls};
use oxy_authz::{RoleCeiling, ServiceAccountStanding, TokenGrant, TokenReach};

const ORG_A: Uuid = Uuid::from_u128(0xA);
const ORG_B: Uuid = Uuid::from_u128(0xB);
const ORG_C: Uuid = Uuid::from_u128(0xC);
const WS_A1: Uuid = Uuid::from_u128(0xA1);
const WS_A2: Uuid = Uuid::from_u128(0xA2);
const WS_B1: Uuid = Uuid::from_u128(0xB1);
const USER: Uuid = Uuid::from_u128(0x5);

fn app(n: u128, org: Uuid, workspace: Uuid, restricted: bool) -> apps::Model {
    let mut row = fixture::app(Uuid::from_u128(n), org, workspace);
    if restricted {
        row.visibility = "members".to_string();
    }
    row
}

fn member_of(orgs: &[Uuid]) -> PrincipalFacts {
    PrincipalFacts {
        user_id: USER,
        member_orgs: orgs.to_vec(),
        ..Default::default()
    }
}

fn kept(facts: &PrincipalFacts, rows: Vec<apps::Model>) -> Vec<u128> {
    keep_openable(facts, rows)
        .iter()
        .map(|r| r.id.as_u128())
        .collect()
}

#[test]
fn a_member_opens_the_unrestricted_apps_of_their_own_org_only() {
    let rows = vec![app(1, ORG_A, WS_A1, false), app(2, ORG_B, WS_B1, false)];
    assert_eq!(kept(&member_of(&[ORG_A]), rows), vec![1]);
}

#[test]
fn a_restricted_app_needs_a_grant() {
    let mut facts = member_of(&[ORG_A]);
    facts.app_memberships = vec![Uuid::from_u128(3)];
    let rows = vec![
        app(1, ORG_A, WS_A1, false),
        app(2, ORG_A, WS_A1, true),
        app(3, ORG_A, WS_A2, true),
    ];
    assert_eq!(kept(&facts, rows), vec![1, 3]);
}

#[test]
fn an_org_officer_keeps_break_glass_on_restricted_apps() {
    let mut facts = member_of(&[ORG_A]);
    facts.admin_orgs = vec![ORG_A];
    assert_eq!(kept(&facts, vec![app(2, ORG_A, WS_A1, true)]), vec![2]);
}

/// A frontline worker is not an org member: an org-wide app is not theirs
/// unless granted, and a grant in an org they are not enrolled in is nothing.
#[test]
fn a_frontline_worker_sees_only_the_apps_granted_to_them() {
    let facts = PrincipalFacts {
        user_id: USER,
        frontline_orgs: vec![ORG_A],
        app_memberships: vec![2, 3, 4].into_iter().map(Uuid::from_u128).collect(),
        ..Default::default()
    };
    let rows = vec![
        app(1, ORG_A, WS_A1, false),
        app(2, ORG_A, WS_A1, false),
        app(3, ORG_A, WS_A2, true),
        app(4, ORG_B, WS_B1, false),
    ];
    assert_eq!(kept(&facts, rows), vec![2, 3]);
    assert_eq!(candidate_orgs(&facts), BTreeSet::from([ORG_A]));
}

#[test]
fn an_org_that_blocked_an_all_access_token_is_left_out() {
    let mut facts = member_of(&[ORG_A, ORG_B]);
    facts.token = Some(TokenReach {
        blocked_orgs: vec![ORG_B],
        ..TokenReach::unrestricted()
    });
    let rows = vec![app(1, ORG_A, WS_A1, false), app(2, ORG_B, WS_B1, false)];
    assert_eq!(kept(&facts, rows), vec![1]);
}

/// Capped at the workspace the app was published from, not by the token's
/// best grant anywhere in the org.
#[test]
fn a_workspace_scoped_token_lists_only_its_workspace() {
    let mut facts = member_of(&[ORG_A]);
    facts.token = Some(TokenReach {
        all_access: false,
        grants: vec![TokenGrant {
            org_id: ORG_A,
            workspace_id: Some(WS_A1),
            ceiling: RoleCeiling::Viewer,
        }],
        ..TokenReach::unrestricted()
    });
    let rows = vec![app(1, ORG_A, WS_A1, false), app(2, ORG_A, WS_A2, false)];
    assert_eq!(kept(&facts, rows), vec![1]);
}

#[test]
fn candidates_are_the_orgs_the_caller_belongs_to_and_no_others() {
    let facts = PrincipalFacts {
        user_id: USER,
        member_orgs: vec![ORG_A],
        frontline_orgs: vec![ORG_B],
        service_account: Some(ServiceAccountStanding {
            org_id: ORG_C,
            admin: false,
        }),
        ..Default::default()
    };
    assert_eq!(
        candidate_orgs(&facts),
        BTreeSet::from([ORG_A, ORG_B, ORG_C])
    );
    // Root standing reaches every tenant, and is still nobody's "my apps".
    let root = PrincipalFacts {
        user_id: USER,
        is_global_owner: true,
        ..Default::default()
    };
    assert!(candidate_orgs(&root).is_empty());
}

// ── Mapping ─────────────────────────────────────────────────────────────────

fn launcher(n: u128, name: &str, org_id: Uuid, org_name: &str) -> LauncherApp {
    let manifest: OxyAppManifest = serde_json::from_value(serde_json::json!({
        "schemaVersion": 2, "slug": "ops", "icon": "icon.svg"
    }))
    .unwrap();
    let build = Some(Uuid::from_u128(0x2a));
    let (icon_url, art_url) = icon_art_urls(Some(&manifest), "acme", "ops", build);
    LauncherApp {
        org_id,
        org_name: org_name.to_string(),
        workspace_id: WS_A1,
        summary: CustomAppSummary {
            id: Uuid::from_u128(n),
            slug: "ops".to_string(),
            name: name.to_string(),
            org_slug: "acme".to_string(),
            url: "/customer-apps/acme/ops/".to_string(),
            published_at: "2026-10-01T00:00:00+00:00".to_string(),
            description: None,
            default_agent: Some("a.agentic.yml".to_string()),
            suggested_questions: vec!["q".to_string()],
            art_url,
            icon_url,
            status: Some("live".to_string()),
            visibility: "org".to_string(),
            verdict: None,
        },
    }
}

/// The exact wire shape: absolute URLs, the `?v=` build pin kept, absent
/// fields as `null`, and nothing launcher-only (agent, chips, status) leaking.
#[test]
fn a_card_maps_to_the_documented_shape_with_absolute_urls() {
    let card = MyApp::from_launcher(
        launcher(1, "Ops", ORG_A, "Acme"),
        None,
        "https://app.example.com/",
    );
    let build = Uuid::from_u128(0x2a);
    assert_eq!(
        serde_json::to_value(&card).unwrap(),
        serde_json::json!({
            "id": Uuid::from_u128(1),
            "slug": "ops",
            "name": "Ops",
            "description": null,
            "org": { "id": ORG_A, "slug": "acme", "name": "Acme" },
            "workspace_id": WS_A1,
            "url": "https://app.example.com/customer-apps/acme/ops/",
            "icon_url": format!("https://app.example.com/customer-apps/acme/ops/icon.svg?v={build}"),
            "art_url": null,
            "published_at": "2026-10-01T00:00:00+00:00",
        })
    );
}

#[test]
fn the_apps_own_host_wins_for_the_app_but_not_for_its_pictures() {
    let host = "https://acme--ops.customer-apps.example.com/".to_string();
    let card = MyApp::from_launcher(
        launcher(1, "Ops", ORG_A, "Acme"),
        Some(host.clone()),
        "http://localhost:3000",
    );
    assert_eq!(card.url, host);
    assert!(
        card.icon_url.as_deref().is_some_and(
            |u| u.starts_with("http://localhost:3000/customer-apps/acme/ops/icon.svg?v=")
        ),
        "{:?}",
        card.icon_url
    );
}

#[test]
fn sorted_by_org_name_then_app_name_ignoring_case() {
    let base = "https://app.example.com";
    let mut cards: Vec<MyApp> = [
        launcher(1, "zeta", ORG_A, "beta"),
        launcher(2, "Bravo", ORG_B, "Alpha"),
        launcher(3, "alpha", ORG_A, "beta"),
        launcher(4, "Charlie", ORG_B, "Alpha"),
    ]
    .into_iter()
    .map(|app| MyApp::from_launcher(app, None, base))
    .collect();
    sort_for_display(&mut cards);
    let names: Vec<&str> = cards.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Bravo", "Charlie", "alpha", "zeta"]);
}
