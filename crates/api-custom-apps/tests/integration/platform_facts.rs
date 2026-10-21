//! `build_platform_facts` over a real Postgres: what `ctx.platform.facts()`
//! will hand an internal app.
//!
//! One world, pinned to Wednesday 2026-10-14 05:00 UTC and read for two
//! completed weeks — W0 from 09-28 and W1 from 10-05 — plus `last7d`, the
//! rolling window from 10-07 05:00. Four things it exists to pin:
//!
//! * **Staff are counted in the totals, not the people.** `sam` is a member of
//!   the internal org and of Acme; his view and thread in Acme count in
//!   `appViews` / `threads` and in none of the `*People` counts.
//! * **Only production counts.** Staging views, invocations and promotes sit
//!   beside every production one and change nothing — not even "last active".
//! * **The windows.** A view on 10-08 belongs to W1 *and* `last7d`; one on
//!   10-06 to W1 only; one on 10-13 to `last7d` only. `weeks` is clamped to
//!   1..=26 end to end.
//! * **`lastActivityAt` is the customer's own.** Sam's production view, an
//!   unattended run and a thread nobody owns are all newer than bob's call,
//!   and none of them moves it. It looks back 180 days and no further
//!   (`horizon`). Per app, `customerLastActiveAt` and `people7d` leave sam
//!   out the same way, while `lastActiveAt` is his.
//!
//! `llmCostUsd` has an org of its own (`llm_cost`).

mod horizon;
mod host_op;
mod llm_cost;
mod world;

use oxy_api_custom_apps::custom_apps_functions::host::platform_facts::OxyPlatformWeek;
use world::{World, at, now, org};

#[tokio::test]
async fn staff_views_count_as_views_but_not_as_people() {
    let w = World::new().await;
    let facts = w.facts().await;
    let w1 = org(&facts, w.acme).weekly[1];
    assert_eq!(w1.app_views, 3, "alice, bob and sam's views all count");
    assert_eq!(w1.app_people, 2, "sam is staff: alice and bob only");
    assert_eq!(w1.threads, 3, "sam's thread counts as a thread");
    assert_eq!(w1.chat_people, 2, "alice and dan, not sam");
    assert_eq!(
        w1.active_people, 3,
        "alice once, bob by view, dan by thread"
    );
}

#[tokio::test]
async fn only_production_counts() {
    let w = World::new().await;
    let facts = w.facts().await;
    let acme = org(&facts, w.acme);
    let w1 = acme.weekly[1];
    assert_eq!(w1.function_calls, 2, "staging invocations are not calls");
    assert_eq!(w1.function_failures, 1);
    assert_eq!(w1.releases, 1, "a staging promote is not a release");
    assert_eq!(w1.agent_runs, 2, "no daemon run, no delegated child");
    assert_eq!(w1.agent_run_failures, 1);
    let ops = acme.apps.iter().find(|a| a.id == w.ops).expect("ops");
    assert_eq!(
        ops.last_active_at,
        Some(at(10, 14, 2)),
        "the app's last production open, staff's included; not the 04:00 staging view"
    );
}

#[tokio::test]
async fn each_app_carries_the_customers_own_use() {
    let w = World::new().await;
    let facts = w.facts().await;
    let acme = org(&facts, w.acme);
    let ops = acme.apps.iter().find(|a| a.id == w.ops).expect("ops");
    assert_eq!(
        ops.last_active_at,
        Some(at(10, 14, 2)),
        "sam's open, staff's included"
    );
    assert_eq!(
        ops.customer_last_active_at,
        Some(at(10, 13, 9)),
        "alice's 10-13 open: not sam's newer one, not the 04:00 staging view"
    );
    assert_eq!(
        ops.people7d, 2,
        "bob's 10-08 and alice's 10-13; sam is staff, alice's 10-06 is before the window"
    );
    let board = acme.apps.iter().find(|a| a.id == w.board).expect("board");
    assert_eq!(
        (board.last_active_at, board.customer_last_active_at),
        (None, None)
    );
    assert_eq!(board.people7d, 0);
}

#[tokio::test]
async fn last_activity_is_the_customers_own() {
    let w = World::new().await;
    let facts = w.facts().await;
    // Newer than bob's 10-13 11:00 call, and none of it the customer's: a
    // nobody's thread at 12:00, sam's (staff) production view at 02:00, an
    // unattended production run at 03:00, a staging view at 04:00.
    assert_eq!(
        org(&facts, w.acme).last_activity_at,
        Some(at(10, 13, 11)),
        "staff, unattended and unattributed activity never make a customer look active"
    );
    assert_eq!(org(&facts, w.beta).last_activity_at, None);
}

#[tokio::test]
async fn weeks_are_clamped_end_to_end() {
    let w = World::new().await;
    let one = w.facts_for(0).await;
    let mondays: Vec<String> = one.weeks.iter().map(|d| d.to_string()).collect();
    assert_eq!(mondays, ["2026-10-05"], "0 asks for at least one week");
    let most = w.facts_for(99).await;
    assert_eq!(most.weeks.len(), 26, "99 is held to 26");
    assert_eq!(most.weeks[0].to_string(), "2026-04-13");
    assert_eq!(most.weeks[25].to_string(), "2026-10-05");
    for (facts, n) in [(&one, 1), (&most, 26)] {
        assert!(facts.orgs.iter().all(|o| o.weekly.len() == n));
    }
    // The newest completed week reads the same whatever the span.
    assert_eq!(org(&most, w.acme).weekly[25], org(&one, w.acme).weekly[0]);
}

#[tokio::test]
async fn weekly_lines_up_with_weeks_and_last7d_is_rolling() {
    let w = World::new().await;
    let facts = w.facts().await;
    assert_eq!(facts.generated_at, now());
    let mondays: Vec<String> = facts.weeks.iter().map(|d| d.to_string()).collect();
    assert_eq!(mondays, ["2026-09-28", "2026-10-05"]);
    for o in &facts.orgs {
        assert_eq!(o.weekly.len(), facts.weeks.len(), "{}", o.name);
    }
    let acme = org(&facts, w.acme);
    assert_eq!(acme.weekly[0].app_views, 1);
    assert_eq!(acme.weekly[0].threads, 0);
    let last7d = acme.last7d;
    assert_eq!(
        last7d.app_views, 3,
        "bob's 10-08, alice's 10-13 and sam's 10-14; not 10-06"
    );
    assert_eq!(last7d.app_people, 2, "bob and alice; sam is staff");
    assert_eq!(
        last7d.threads, 2,
        "dan's 10-12 and the 10-13 thread nobody owns"
    );
    assert_eq!(
        last7d.chat_people, 1,
        "dan; nobody's thread names no person"
    );
    assert_eq!(
        last7d.active_people, 3,
        "bob and alice by view, dan by thread"
    );
    assert_eq!(
        last7d.function_calls, 2,
        "bob's 10-13 call and the 10-14 run"
    );
    assert_eq!(last7d.releases, 0);
    let beta = org(&facts, w.beta);
    assert!(
        beta.weekly
            .iter()
            .all(|wk| *wk == OxyPlatformWeek::default())
    );
    assert_eq!(beta.last7d, OxyPlatformWeek::default());
}

#[tokio::test]
async fn only_the_internal_org_is_internal_and_the_snapshot_is_whole() {
    let w = World::new().await;
    let facts = w.facts().await;
    assert!(org(&facts, w.staff_org).internal);
    for id in [w.acme, w.beta, w.partner] {
        assert!(!org(&facts, id).internal);
    }
    let order: Vec<&str> = facts
        .orgs
        .iter()
        .filter(|o| [w.staff_org, w.acme, w.beta, w.partner].contains(&o.id))
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(order, ["Acme", "Beta", "Oxygen", "Partner Co"]);

    let acme = org(&facts, w.acme);
    assert_eq!((acme.members, acme.workspaces), (4, 1));
    assert_eq!(
        acme.last_sign_in_at,
        Some(at(10, 14, 2)),
        "any member's, sam's included"
    );
    let names: Vec<&str> = acme.apps.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Board", "Ops"]);
    assert_eq!(acme.apps[0].id, w.board);
    assert_eq!(acme.apps[0].published_at, None);
    assert_eq!(acme.milestones.first_workspace_at, Some(at(9, 1, 0)));
    assert_eq!(acme.milestones.first_app_published_at, Some(at(9, 3, 0)));
    assert_eq!(acme.milestones.first_thread_at, Some(at(9, 15, 10)));
    assert_eq!(acme.billing.status, None);

    let beta = org(&facts, w.beta);
    assert_eq!(beta.partner_org_id, Some(w.partner));
    assert_eq!(beta.billing.status.as_deref(), Some("past_due"));
    assert_eq!(beta.billing.seats_paid, Some(5));
    assert_eq!(beta.billing.grace_ends_at, Some(at(10, 20, 0)));
    assert_eq!((beta.members, beta.workspaces), (1, 0));
    assert!(beta.apps.is_empty());
    assert_eq!(beta.last_activity_at, None);
    assert_eq!(beta.milestones.first_thread_at, None);
}
