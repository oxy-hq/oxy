//! The fixture's pure parts: the bundles, the calendar and the seeded history.
//! The release script and the writers are `tests/integration/demo_fixture.rs`.

use std::collections::{BTreeSet, HashSet};

use chrono::{Duration, TimeZone, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::bundle::{RELEASES, releases};
use super::calendar::{Calendar, LIVE};
use super::history::{Seeded, invocations};
use super::vitals::{MIN_SESSIONS, SCREENS, VITALS_KEYS, page_loads, sessions_in};
use crate::custom_apps_build_store::is_valid_build_id;
use crate::custom_apps_validate::validate_bundle;

const RUNTIME_JS: &str = include_str!("../client/runtime.js");

fn t() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 21, 9, 30, 0).unwrap()
}

fn app() -> Uuid {
    super::fixture_app_id(Uuid::nil())
}

fn builds() -> Vec<Uuid> {
    (1..=RELEASES)
        .map(|k| Uuid::new_v5(&Uuid::nil(), format!("build-{k}").as_bytes()))
        .collect()
}

fn text(release: &super::Release, path: &str) -> String {
    String::from_utf8(release.files[path].clone()).expect("UTF-8")
}

// ── bundles ─────────────────────────────────────────────────────────────────

#[test]
fn every_release_passes_gate_one_and_ships_exactly_the_functions_it_declares() {
    for r in releases() {
        let files: Vec<(String, Vec<u8>)> = r.files.clone().into_iter().collect();
        validate_bundle(&files, "local", super::APP_SLUG)
            .unwrap_or_else(|e| panic!("{}: {e:?}", r.label()));
        let shipped: BTreeSet<String> = r
            .files
            .keys()
            .filter_map(|p| p.strip_prefix("functions/")?.strip_suffix(".js"))
            .map(str::to_string)
            .collect();
        let declared: BTreeSet<String> = r.functions().into_iter().collect();
        assert_eq!(shipped, declared, "{}", r.label());
        assert_eq!(r.manifest()["slug"], super::APP_SLUG);
    }
}

#[test]
fn releases_are_five_distinct_valid_build_ids_in_order() {
    let all = releases();
    assert_eq!(all.len(), 5);
    let ids: HashSet<&str> = all.iter().map(|r| r.build_id.as_str()).collect();
    assert_eq!(
        ids.len(),
        5,
        "an overlay that changed nothing would repeat an id"
    );
    for (k, r) in all.iter().enumerate() {
        assert!(is_valid_build_id(&r.build_id), "{}", r.build_id);
        assert!(
            r.build_id.starts_with(&format!("pulse-r{}-", k + 1)),
            "{}",
            r.build_id
        );
    }
}

#[test]
fn every_asset_is_named_by_its_own_content_and_index_html_names_them() {
    for r in releases() {
        let html = text(&r, "index.html");
        let assets: Vec<&String> = r
            .files
            .keys()
            .filter(|p| p.starts_with("assets/"))
            .collect();
        assert_eq!(assets.len(), 2, "{}", r.label());
        for path in assets {
            let sha = hex::encode(Sha256::digest(&r.files[path]));
            assert!(
                path.contains(&format!("-{}.", &sha[..10])),
                "{path} is not its own hash"
            );
            assert!(
                html.contains(&format!("\"{path}\"")),
                "index.html does not load {path}"
            );
        }
        assert!(!html.contains("\"assets/app.js\"") && !html.contains("\"assets/app.css\""));
        assert!(html.contains(&format!("content=\"{}\"", r.label())));
        let functions = r.functions().join(",");
        assert!(html.contains(&format!("name=\"pulse-functions\" content=\"{functions}\"")));
    }
}

/// Every `ctx.<power>` a function's JS calls has its capability in the
/// manifest (they fail closed), and nothing reaches OLTP.
#[test]
fn every_power_a_function_uses_is_declared_and_none_is_oltp() {
    let reads = ["ctx.storage.get(", "ctx.storage.head(", "ctx.storage.list("];
    let writes = [
        "ctx.storage.put(",
        "ctx.storage.delete(",
        "ctx.storage.copy(",
    ];
    for r in releases() {
        let manifest = r.manifest();
        assert!(manifest.get("migrations").is_none() && manifest.get("oltpMigrations").is_none());
        for name in r.functions() {
            let js = text(&r, &format!("functions/{name}.js"));
            let decl = &manifest["functions"][&name];
            let at = format!("{} {name}", r.label());
            assert!(
                !js.contains("ctx.oltp") && decl.get("oltp").is_none(),
                "{at}: OLTP"
            );
            if reads.iter().any(|op| js.contains(op)) {
                assert_eq!(decl["storage"]["read"], true, "{at}: storage.read");
            }
            if writes.iter().any(|op| js.contains(op)) {
                assert_eq!(decl["storage"]["write"], true, "{at}: storage.write");
            }
            if js.contains("ctx.email.send(") {
                assert_eq!(decl["email"]["send"], true, "{at}: email.send");
            }
        }
    }
}

#[test]
fn r4_is_the_release_that_drops_the_store_guard_and_r5_restores_it() {
    let all = releases();
    let store = |k: usize| text(&all[k - 1], "functions/store.js");
    for k in [1, 2, 3, 5] {
        assert!(
            store(k).contains("status: 404"),
            "r{k} answers a stale id with a 404"
        );
    }
    assert!(!store(LIVE).contains("status: 404"));
    assert!(
        store(LIVE).contains(
            "const store = summary.rows[0];\n  const rank = rankIn(snapshot, store.store);"
        )
    );
}

// ── calendar ────────────────────────────────────────────────────────────────

#[test]
fn the_calendar_runs_forward_over_seven_days_and_r5_is_staging_only() {
    let cal = Calendar::at(t());
    assert!(cal.publish.windows(2).all(|w| w[0] < w[1]));
    for k in 0..RELEASES {
        if let Some(promoted) = cal.promote[k] {
            assert!(
                promoted > cal.publish[k],
                "r{} promoted before it was published",
                k + 1
            );
        }
    }
    assert_eq!(cal.promote[RELEASES - 1], None);
    assert_eq!(cal.t - cal.publish[0], Duration::days(7));
    assert_eq!(cal.t - cal.promote[LIVE - 1].unwrap(), Duration::hours(6));

    let stretches = cal.stretches();
    assert_eq!(
        stretches.iter().map(|s| s.release).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(stretches.windows(2).all(|w| w[0].end == w[1].start));
    assert_eq!(stretches.last().unwrap().end, cal.t);
}

#[test]
fn the_calendar_is_cut_to_the_minute() {
    let cal = Calendar::at(t() + Duration::seconds(42) + Duration::milliseconds(7));
    assert_eq!(cal, Calendar::at(t()));
}

// ── invocation history ──────────────────────────────────────────────────────

#[test]
fn the_history_is_deterministic_and_its_ids_are_unique() {
    let viewer = Uuid::nil();
    let a = invocations(app(), &Calendar::at(t()), viewer);
    assert_eq!(a, invocations(app(), &Calendar::at(t()), viewer));
    let ids: HashSet<Uuid> = a.iter().map(|r| r.id).collect();
    assert_eq!(ids.len(), a.len());
    let other = invocations(Uuid::new_v4(), &Calendar::at(t()), viewer);
    assert!(other.iter().all(|r| !ids.contains(&r.id)));
    let cal = Calendar::at(t());
    for (r, s) in a.iter().map(|r| (r, cal.stretches()[r.release - 1])) {
        assert!(r.at >= s.start && r.at < s.end, "a row outside its stretch");
    }
}

/// The release timeline's rule (plan-timeline §2, restated here until its
/// constants exist): failure rate ≥ 2× and +1 pt with ≥ 50 calls each side and
/// ≥ 3 failed now; route p95 ≥ 1.5× and +200 ms with ≥ 50 route calls each side.
struct Side {
    calls: usize,
    failed: usize,
    route: Vec<i64>,
}

fn side(rows: &[Seeded], release: usize) -> Side {
    let mine: Vec<&Seeded> = rows.iter().filter(|r| r.release == release).collect();
    let mut route: Vec<i64> = mine
        .iter()
        .filter(|r| r.mode == "route")
        .map(|r| r.duration_ms)
        .collect();
    route.sort_unstable();
    Side {
        calls: mine.len(),
        failed: mine
            .iter()
            .filter(|r| r.status != "success" || r.fingerprint.is_some())
            .count(),
        route,
    }
}

/// Postgres's `percentile_cont`.
fn percentile(sorted: &[i64], p: f64) -> f64 {
    let at = p * (sorted.len() - 1) as f64;
    let (lo, hi) = (at.floor() as usize, at.ceil() as usize);
    sorted[lo] as f64 + (sorted[hi] - sorted[lo]) as f64 * (at - lo as f64)
}

fn worse(cur: &Side, prev: &Side) -> (bool, bool) {
    let rate = |s: &Side| s.failed as f64 / s.calls as f64;
    let failure = cur.calls >= 50
        && prev.calls >= 50
        && cur.failed >= 3
        && rate(cur) >= 2.0 * rate(prev)
        && rate(cur) - rate(prev) >= 0.01;
    let (p_cur, p_prev) = (percentile(&cur.route, 0.95), percentile(&prev.route, 0.95));
    let p95 = cur.route.len() >= 50
        && prev.route.len() >= 50
        && p_cur >= 1.5 * p_prev
        && p_cur - p_prev >= 200.0;
    (failure, p95)
}

#[test]
fn r4_is_worse_than_r3_on_both_rules_and_no_earlier_release_is_worse() {
    let rows = invocations(app(), &Calendar::at(t()), Uuid::nil());
    let sides: Vec<Side> = (1..=LIVE).map(|k| side(&rows, k)).collect();
    assert_eq!(worse(&sides[3], &sides[2]), (true, true), "r4 against r3");
    for k in 1..3 {
        assert_eq!(
            worse(&sides[k], &sides[k - 1]),
            (false, false),
            "r{} against r{k}",
            k + 1
        );
    }
}

#[test]
fn the_only_failure_in_the_history_is_r4s_one_fingerprint() {
    let rows = invocations(app(), &Calendar::at(t()), Uuid::nil());
    let failed: Vec<&Seeded> = rows.iter().filter(|r| r.status != "success").collect();
    assert!(failed.len() >= 3);
    assert!(
        failed
            .iter()
            .all(|r| r.release == LIVE && r.function == "store")
    );
    let prints: HashSet<&Option<String>> = rows.iter().map(|r| &r.fingerprint).collect();
    assert_eq!(prints.len(), 2, "None and r4's one fingerprint: {prints:?}");
    let print = failed[0]
        .fingerprint
        .clone()
        .expect("a failed row is fingerprinted");
    assert_eq!(
        Some(print),
        crate::custom_apps_functions::seeded_fingerprint("error", 0, Some(super::R4_STORE_ERROR))
    );
}

#[test]
fn route_rows_name_the_viewer_and_system_rows_name_nobody() {
    let viewer = Uuid::new_v4();
    for r in invocations(app(), &Calendar::at(t()), viewer) {
        match r.mode {
            "route" => assert_eq!(r.user, Some(viewer)),
            "schedule" | "manual" => assert_eq!(r.user, None),
            other => panic!("unexpected mode {other}"),
        }
    }
}

// ── vitals ──────────────────────────────────────────────────────────────────

#[test]
fn every_screen_has_enough_page_loads_in_every_stretch() {
    let cal = Calendar::at(t());
    let loads = page_loads(app(), &cal, &builds());
    assert!(MIN_SESSIONS >= 30);
    for s in cal.stretches() {
        for screen in SCREENS {
            let n = loads
                .iter()
                .filter(|l| l.release == s.release && l.screen == screen)
                .count();
            assert_eq!(n, sessions_in(&s));
            assert!(n >= MIN_SESSIONS, "r{} {screen}: {n}", s.release);
        }
    }
}

#[test]
fn a_vitals_payload_has_the_runtimes_keys_plus_its_screen_and_build() {
    for key in &VITALS_KEYS[..6] {
        assert!(
            RUNTIME_JS.contains(&format!("vitals.{key} =")),
            "runtime.js sends no {key}"
        );
    }
    let cal = Calendar::at(t());
    let builds = builds();
    for load in page_loads(app(), &cal, &builds) {
        let keys: BTreeSet<&str> = load
            .vitals
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, VITALS_KEYS.iter().copied().collect::<BTreeSet<_>>());
        assert_eq!(load.vitals["build"], builds[load.release - 1].to_string());
        assert_eq!(load.vitals["path"], load.screen);
        let s = cal.stretches()[load.release - 1];
        assert!(load.at >= s.start && load.at + Duration::seconds(10) < s.end);
    }
}

#[test]
fn r4_regresses_the_store_screens_lcp_and_nothing_else_moves_much() {
    let cal = Calendar::at(t());
    let loads = page_loads(app(), &cal, &builds());
    let p75 = |release: usize, screen: &str| {
        let mut v: Vec<i64> = loads
            .iter()
            .filter(|l| l.release == release && l.screen == screen)
            .map(|l| l.vitals["lcp"].as_i64().unwrap())
            .collect();
        v.sort_unstable();
        percentile(&v, 0.75)
    };
    assert!(p75(4, "/store") >= 1.25 * p75(3, "/store") + 500.0);
    assert!(p75(4, "/") < 1.25 * p75(3, "/"));
    assert!(p75(3, "/store") < 1.25 * p75(2, "/store"));
}

#[test]
fn the_fixture_id_is_derived_and_is_never_a_seeded_example_apps() {
    let org = Uuid::new_v4();
    assert_eq!(super::fixture_app_id(org), super::fixture_app_id(org));
    assert_ne!(
        super::fixture_app_id(org),
        super::fixture_app_id(Uuid::new_v4())
    );
    assert_ne!(
        super::fixture_app_id(org),
        Uuid::new_v5(
            &Uuid::NAMESPACE_DNS,
            format!("oxy.app-seed.{org}.{}", super::APP_SLUG).as_bytes()
        )
    );
}

#[test]
fn the_sentinel_is_the_last_event_written_whatever_the_seed_time() {
    let builds = builds();
    for at in [
        t(),
        t() + Duration::minutes(7),
        t() + Duration::days(3) + Duration::minutes(44),
    ] {
        let last = page_loads(app(), &Calendar::at(at), &builds).pop().unwrap();
        let id = Uuid::new_v5(&last.session, b"oxy-web-vitals");
        assert_eq!(
            super::vitals::sentinel_event_id(app()),
            id,
            "seeded at {at}"
        );
    }
}
