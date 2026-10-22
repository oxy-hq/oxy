//! Each threshold of the rule just under and just over its boundary, every
//! case with explicit counts, and each arm of a two-armed rule isolated.

use super::*;

fn calls(failed: i64, calls: i64) -> Side {
    Side {
        calls,
        failed,
        ..Side::default()
    }
}

fn timed(p95: i64) -> Side {
    Side {
        route_calls: 100,
        p95_ms: Some(p95),
        ..Side::default()
    }
}

fn screen(path: &str, samples: i64, lcp: i64, cls: i64) -> Screen {
    Screen {
        path: path.into(),
        samples,
        lcp_p75: Some(lcp),
        cls_p75: Some(cls),
    }
}

fn paged(screens: Vec<Screen>) -> Side {
    Side {
        screens,
        ..Side::default()
    }
}

fn kind(cur: &Side, prev: &Side) -> Kind {
    judge(cur, prev).kind
}

#[test]
fn three_failures_after_a_clean_release_is_red_and_two_is_not() {
    let clean = calls(0, 100);
    let red = judge(&calls(3, 60), &clean);
    assert_eq!(red.kind, Kind::Worse);
    assert_eq!(red.reasons[0].metric, "failure_rate");
    assert_eq!((red.reasons[0].before, red.reasons[0].after), (0, 500));
    assert_eq!(red.reasons[0].unit, "bp");
    // Two of 60 is 3.3%, over both arms, but under MIN_FAILED.
    assert_eq!(kind(&calls(2, 60), &clean), Kind::NotWorse);
}

#[test]
fn exactly_twice_the_rate_and_exactly_one_point_higher_is_red() {
    // 1.0% → 2.0%: both arms on their boundary, inclusive.
    assert_eq!(kind(&calls(4, 200), &calls(2, 200)), Kind::Worse);
}

#[test]
fn a_rise_of_more_than_a_point_but_under_twice_the_rate_is_not_red() {
    // 2.0% → 3.9%: +1.9 points, 1.95×. Only the 2× arm holds it back.
    assert_eq!(kind(&calls(39, 1000), &calls(4, 200)), Kind::NotWorse);
    // 2.0% → 4.0% fires: the same pair with the 2× arm met.
    assert_eq!(kind(&calls(40, 1000), &calls(4, 200)), Kind::Worse);
}

#[test]
fn twice_the_rate_but_under_a_point_higher_is_not_red() {
    // 0.4% → 0.9%: 2.25×, +0.5 points. Only the 1-point arm holds it back.
    assert_eq!(kind(&calls(9, 1000), &calls(4, 1000)), Kind::NotWorse);
    // 0.4% → 1.4% fires: +1.0 point.
    assert_eq!(kind(&calls(14, 1000), &calls(4, 1000)), Kind::Worse);
}

#[test]
fn under_fifty_calls_on_either_side_is_not_comparable_and_says_why() {
    let v = judge(&calls(10, 49), &calls(0, 100));
    assert_eq!(v.kind, Kind::NotComparable);
    assert_eq!(v.why.as_deref(), Some("too few calls: 49 of 50"));
    let v = judge(&calls(10, 100), &calls(0, 49));
    assert_eq!(v.kind, Kind::NotComparable);
    // Fifty is enough.
    assert_eq!(kind(&calls(10, 50), &calls(0, 50)), Kind::Worse);
}

#[test]
fn p95_is_amber_at_one_and_a_half_times_and_two_hundred_ms() {
    let v = judge(&timed(750), &timed(500));
    assert_eq!(v.kind, Kind::Slower, "latency never makes a release red");
    assert_eq!((v.reasons[0].before, v.reasons[0].after), (500, 750));
    // 500 → 740: +240 ms, but 1.48×. Only the factor holds it back.
    assert_eq!(kind(&timed(740), &timed(500)), Kind::NotWorse);
    // 300 → 499: 1.66×, but +199 ms. Only the floor holds it back.
    assert_eq!(kind(&timed(499), &timed(300)), Kind::NotWorse);
    assert_eq!(kind(&timed(500), &timed(300)), Kind::Slower);
}

#[test]
fn p95_needs_fifty_route_calls_on_each_side() {
    let few = Side {
        route_calls: 49,
        ..timed(2000)
    };
    let v = judge(&few, &timed(100));
    assert_eq!(v.kind, Kind::NotComparable);
}

#[test]
fn vitals_alone_are_amber_never_red() {
    let before = paged(vec![screen("/store", 36, 1000, 10)]);
    let after = paged(vec![screen("/store", 36, 1500, 10)]);
    let v = judge(&after, &before);
    assert_eq!(v.kind, Kind::Slower);
    assert_eq!(v.reasons[0].metric, "lcp");
    assert_eq!(v.reasons[0].screen.as_deref(), Some("/store"));
}

#[test]
fn lcp_needs_a_quarter_more_and_five_hundred_ms() {
    let at = |lcp| paged(vec![screen("/", 40, lcp, 0)]);
    assert_eq!(kind(&at(2500), &at(2000)), Kind::Slower);
    // +499 ms at exactly 1.25×… is 2499 / 2000 = 1.2495: both arms short by one.
    assert_eq!(kind(&at(2499), &at(2000)), Kind::NotWorse);
    // 1000 → 1249: +249 ms and 1.249×.
    assert_eq!(kind(&at(1249), &at(1000)), Kind::NotWorse);
    // 3000 → 3600: +600 ms, 1.2×. Only the factor holds it back.
    assert_eq!(kind(&at(3600), &at(3000)), Kind::NotWorse);
    // 600 → 1000: 1.67×, +400 ms. Only the floor holds it back.
    assert_eq!(kind(&at(1000), &at(600)), Kind::NotWorse);
}

#[test]
fn cls_needs_a_rise_of_fifty_and_to_end_over_one_hundred() {
    let at = |cls| paged(vec![screen("/", 40, 1000, cls)]);
    let v = judge(&at(110), &at(60));
    assert_eq!(v.kind, Kind::Slower);
    assert_eq!(v.reasons[0].unit, "cls_milli");
    // +50 but ends at 90: still good.
    assert_eq!(kind(&at(90), &at(40)), Kind::NotWorse);
    // Ends at 101 but rose 49.
    assert_eq!(kind(&at(101), &at(52)), Kind::NotWorse);
}

#[test]
fn a_screen_under_twenty_page_loads_on_either_side_is_not_judged() {
    let before = paged(vec![screen("/", 19, 1000, 0)]);
    let after = paged(vec![screen("/", 40, 5000, 0)]);
    assert_eq!(kind(&after, &before), Kind::NotComparable);
    // A screen only one side has is not judged either.
    let other = paged(vec![screen("/other", 40, 1000, 0)]);
    assert_eq!(kind(&after, &other), Kind::NotComparable);
}

#[test]
fn failure_and_latency_firing_together_give_red_with_both_reasons() {
    let prev = Side {
        calls: 200,
        failed: 0,
        ..timed(300)
    };
    let cur = Side {
        calls: 100,
        failed: 10,
        ..timed(900)
    };
    let v = judge(&cur, &prev);
    assert_eq!(v.kind, Kind::Worse);
    let metrics: Vec<_> = v.reasons.iter().map(|r| r.metric).collect();
    assert_eq!(metrics, ["failure_rate", "p95"]);
}

#[test]
fn nothing_measured_on_either_side_is_never_not_worse() {
    let v = judge(&Side::default(), &Side::default());
    assert_eq!(v.kind, Kind::NotComparable);
    assert_eq!(v.why.as_deref(), Some("too few calls: 0 of 50"));
}

#[test]
fn rates_are_integer_basis_points() {
    assert_eq!(rate_bp(19, 112), Some(1696));
    assert_eq!(rate_bp(0, 926), Some(0));
    assert_eq!(rate_bp(0, 0), None);
}
