use super::*;
use chrono::TimeZone;

fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

const ORG_A: &str = "0f8e2b9c-3a51-4d6e-9c1f-2b7a4e5d6c01";
const ORG_B: &str = "7c2d1e4f-5a6b-4c3d-8e9f-0a1b2c3d4e02";

// Parsing the list is `oxy_app_core::internal_orgs`'s, and tested there.

#[test]
fn the_gate_fails_closed() {
    let org = Uuid::parse_str(ORG_A).unwrap();
    assert!(!facts_enabled_for(org, &[]));
    assert!(!facts_enabled_for(org, &[Uuid::parse_str(ORG_B).unwrap()]));
    assert!(facts_enabled_for(org, &[org]));
}

#[test]
fn weeks_are_held_to_one_through_twenty_six() {
    assert_eq!(clamp_weeks(0), 1);
    assert_eq!(clamp_weeks(DEFAULT_WEEKS), 8);
    assert_eq!(clamp_weeks(500), MAX_WEEKS);
}

#[test]
fn completed_weeks_end_before_the_week_in_progress() {
    // 2026-10-14 is a Wednesday: its own week (from 10-12) has not ended.
    assert_eq!(
        completed_week_starts(at(2026, 10, 14, 5), 2),
        vec![day(2026, 9, 28), day(2026, 10, 5)]
    );
}

#[test]
fn at_monday_midnight_the_week_just_ended_is_complete() {
    assert_eq!(
        completed_week_starts(at(2026, 10, 12, 0), 1),
        vec![day(2026, 10, 5)]
    );
}

#[test]
fn the_last_window_is_the_rolling_seven_days() {
    let now = at(2026, 10, 14, 5);
    let windows = Windows::new(&completed_week_starts(now, 2), now);
    assert_eq!(windows.len(), 3);
    assert_eq!(windows.starts[0], at(2026, 9, 28, 0));
    assert_eq!(windows.ends[1], at(2026, 10, 12, 0));
    assert_eq!(windows.starts[2], at(2026, 10, 7, 5));
    assert_eq!(windows.ends[2], now);
    assert_eq!(windows.lo(), at(2026, 9, 28, 0));
    assert_eq!(windows.hi(), now);
}

#[test]
fn the_json_carries_the_contract_names() {
    let now = at(2026, 10, 14, 5);
    let org = OxyPlatformOrg {
        id: Uuid::parse_str(ORG_A).unwrap(),
        slug: "acme".into(),
        name: "Acme".into(),
        created_at: now,
        internal: false,
        partner_org_id: None,
        members: 3,
        workspaces: 1,
        billing: OxyPlatformBilling::default(),
        last_sign_in_at: None,
        last_activity_at: Some(now),
        apps: vec![OxyPlatformApp {
            id: Uuid::parse_str(ORG_B).unwrap(),
            slug: "ops".into(),
            name: "Ops".into(),
            published_at: None,
            last_active_at: None,
            customer_last_active_at: None,
            people7d: 0,
        }],
        milestones: OxyPlatformMilestones::default(),
        weekly: vec![OxyPlatformWeek::default()],
        last7d: OxyPlatformWeek::default(),
    };
    let facts = OxyPlatformFacts {
        generated_at: now,
        weeks: vec![day(2026, 10, 5)],
        orgs: vec![org],
    };
    let json = serde_json::to_value(&facts).unwrap();
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&json), ["generatedAt", "orgs", "weeks"]);
    assert_eq!(json["weeks"][0], "2026-10-05");
    assert_eq!(json["generatedAt"], "2026-10-14T05:00:00Z");
    let org = &json["orgs"][0];
    assert_eq!(
        keys(org),
        [
            "apps",
            "billing",
            "createdAt",
            "id",
            "internal",
            "last7d",
            "lastActivityAt",
            "lastSignInAt",
            "members",
            "milestones",
            "name",
            "partnerOrgId",
            "slug",
            "weekly",
            "workspaces",
        ]
    );
    assert!(org["partnerOrgId"].is_null(), "null, not absent");
    assert_eq!(
        keys(&org["billing"]),
        ["graceEndsAt", "seatsPaid", "status"]
    );
    assert_eq!(
        keys(&org["apps"][0]),
        [
            "customerLastActiveAt",
            "id",
            "lastActiveAt",
            "name",
            "people7d",
            "publishedAt",
            "slug",
        ]
    );
    assert_eq!(
        keys(&org["milestones"]),
        ["firstAppPublishedAt", "firstThreadAt", "firstWorkspaceAt"]
    );
    assert_eq!(
        keys(&org["last7d"]),
        [
            "activePeople",
            "agentRunFailures",
            "agentRuns",
            "appPeople",
            "appViews",
            "chatPeople",
            "functionCalls",
            "functionFailures",
            "llmCostUsd",
            "llmUnpricedRuns",
            "releases",
            "threads",
        ]
    );
    assert!(
        org["last7d"]["llmCostUsd"].is_number(),
        "a number, never null"
    );
    assert!(
        org["apps"][0]["customerLastActiveAt"].is_null(),
        "null, not absent"
    );
}
