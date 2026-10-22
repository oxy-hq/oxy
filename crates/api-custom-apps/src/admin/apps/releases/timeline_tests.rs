use chrono::{Duration, TimeZone};

use super::*;

fn t(day: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap() + Duration::days(day)
}

fn ev(day: i64, action: &str, build: Option<Uuid>) -> Event {
    Event {
        id: Uuid::new_v4(),
        environment: "production".into(),
        action: action.into(),
        at: t(day),
        build_id: build,
        actor: None,
    }
}

fn ids() -> (Uuid, Uuid, Uuid) {
    (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
}

#[test]
fn the_anchor_is_clipped_to_the_window_and_the_last_release_is_current() {
    let (a, b, _) = ids();
    let s = stretches(
        Some(ev(1, "promote", Some(a))),
        vec![ev(5, "promote", Some(b))],
        t(3),
        t(10),
        false,
    );
    assert_eq!(s.len(), 2);
    assert!(s[0].clipped);
    assert_eq!(
        (s[0].began_at, s[0].starts_at, s[0].ends_at),
        (t(1), t(3), t(5))
    );
    assert!(!s[0].current);
    assert!(!s[1].clipped);
    assert!(s[1].current);
    assert_eq!(s[1].ends_at, t(10));
    assert_eq!(s[1].previous, Some(0));
}

#[test]
fn a_re_promote_of_the_serving_build_is_not_a_release() {
    let (a, b, _) = ids();
    let s = stretches(
        None,
        vec![
            ev(1, "promote", Some(a)),
            ev(2, "promote", Some(a)),
            ev(3, "promote", Some(a)),
            ev(4, "promote", Some(b)),
        ],
        t(0),
        t(10),
        false,
    );
    assert_eq!(s.len(), 2, "{s:#?}");
    assert_eq!(s[0].ends_at, t(4));
    assert_eq!(s[1].previous, Some(0));
}

#[test]
fn an_unpublish_is_a_gap_that_previous_skips() {
    let (a, b, _) = ids();
    let s = stretches(
        None,
        vec![
            ev(1, "promote", Some(a)),
            ev(2, "unpublish", None),
            ev(3, "promote", Some(b)),
        ],
        t(0),
        t(10),
        false,
    );
    assert_eq!(s.len(), 3);
    assert!(s[1].unpublished && !s[1].pruned);
    assert_eq!(s[1].previous, None);
    assert_eq!(s[2].previous, Some(0));
}

#[test]
fn a_pruned_build_is_marked_and_never_a_previous() {
    let (a, b, _) = ids();
    let s = stretches(
        None,
        vec![
            ev(1, "promote", Some(a)),
            ev(2, "promote", None),
            ev(3, "promote", Some(b)),
        ],
        t(0),
        t(10),
        false,
    );
    assert!(s[1].pruned);
    assert_eq!(s[1].previous, None);
    assert_eq!(
        s[2].previous,
        Some(0),
        "b is compared past the pruned build"
    );
}

#[test]
fn a_rollback_to_an_earlier_build_is_a_new_release_of_it() {
    let (a, b, c) = ids();
    let s = stretches(
        None,
        vec![
            ev(1, "promote", Some(a)),
            ev(2, "promote", Some(b)),
            ev(3, "rollback", Some(a)),
            ev(4, "rollback", Some(c)),
        ],
        t(0),
        t(10),
        false,
    );
    assert_eq!(s.len(), 4);
    assert_eq!(s[2].build_id, Some(a));
    assert_eq!(s[2].started_by, StartedBy::Rollback);
    assert_eq!(s[2].previous, Some(1));
    // A "rollback" to a build production never served is the console's
    // Make live: a forward move.
    assert_eq!(s[3].started_by, StartedBy::MadeLive);
}

#[test]
fn previous_is_never_the_same_build() {
    let (a, _, _) = ids();
    let s = stretches(
        None,
        vec![
            ev(1, "promote", Some(a)),
            ev(2, "unpublish", None),
            ev(3, "promote", Some(a)),
        ],
        t(0),
        t(10),
        false,
    );
    assert_eq!(s[2].previous, None);
}

#[test]
fn a_backfill_is_where_history_starts_and_still_a_previous() {
    let (a, b, _) = ids();
    let s = stretches(
        None,
        vec![ev(1, "backfill", Some(a)), ev(2, "promote", Some(b))],
        t(0),
        t(10),
        false,
    );
    assert_eq!(s[0].started_by, StartedBy::HistoryStart);
    assert_eq!(s[1].previous, Some(0));
}

#[test]
fn a_cut_read_clips_its_oldest_event_and_ignores_the_anchor() {
    let (a, b, c) = ids();
    let s = stretches(
        Some(ev(1, "promote", Some(c))),
        vec![ev(5, "promote", Some(a)), ev(6, "promote", Some(b))],
        t(3),
        t(10),
        true,
    );
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].build_id, Some(a));
    assert!(s[0].clipped);
    assert!(s[1].current, "the release serving now is never cut");
}

#[test]
fn no_events_and_no_anchor_is_no_release() {
    assert!(stretches(None, vec![], t(0), t(10), false).is_empty());
}

#[test]
fn a_migration_goes_to_the_release_that_began_after_it_ran() {
    let (a, b, c) = ids();
    let s = stretches(
        None,
        vec![
            ev(1, "promote", Some(a)),
            ev(3, "promote", Some(b)),
            ev(5, "rollback", Some(a)),
            ev(7, "rollback", Some(b)),
        ],
        t(0),
        t(10),
        false,
    );
    let row = |day, build| Applied {
        store: "oltp".into(),
        filename: format!("{day}.sql"),
        applied_at: t(day) - Duration::minutes(1),
        build_id: build,
    };
    let (per, unreleased) = attribute_migrations(
        &s,
        vec![
            row(3, Some(b)),
            row(4, Some(c)),
            row(9, Some(a)),
            row(6, None),
        ],
    );
    assert_eq!(per[1].len(), 1, "b's file lands on b's first release");
    assert!(per[3].is_empty(), "not again on b's second");
    let unreleased: Vec<_> = unreleased.iter().map(|r| r.filename.as_str()).collect();
    // c never went live; a's file ran after a's last release began; the
    // fourth row's build is gone.
    assert_eq!(unreleased, ["4.sql", "9.sql", "6.sql"]);
}

#[test]
fn a_new_failure_is_attributed_only_on_the_build_it_failed_on() {
    let (a, b, _) = ids();
    let s = stretches(
        None,
        vec![ev(1, "promote", Some(a)), ev(5, "promote", Some(b))],
        t(0),
        t(10),
        false,
    );
    let seen = |day, build| FirstSeen {
        function_name: "store".into(),
        fingerprint: format!("f{day}"),
        build_id: build,
        first_seen: t(day),
        occurrences: 3,
    };
    // f6 on b during b: b's. f2 on a during a: a's. f6b on a after the move:
    // a call still running on the old build belongs to neither.
    let per = attribute_issues(&s, vec![seen(6, b), seen(2, a), seen(7, a)]);
    assert_eq!(per[1].len(), 1);
    assert_eq!(per[1][0].fingerprint, "f6");
    assert_eq!(per[0].len(), 1);
    assert_eq!(per[0][0].fingerprint, "f2");
}
