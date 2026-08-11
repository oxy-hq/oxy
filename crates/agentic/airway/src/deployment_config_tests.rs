//! Unit tests for the row → [`GlobalConfig`] mapping and the drift
//! comparison. No database and no HTTP: every one of these is a property of
//! the mapping itself, which is the layer the three "absence is not zero"
//! regressions kept landing in.

use std::cell::RefCell;

use airway::connector::http::HttpConfig;
use airway::destination::retry::RetryConfig;

use super::*;

/// Everything set, so a test can knock one field out rather than build one up.
fn fully_configured() -> DeploymentValues {
    DeploymentValues {
        timeout_secs: Some(90),
        max_retries: Some(7),
        user_agent: Some("oxy-airway/1.0".into()),
        retry_initial_delay_ms: Some(250),
        retry_max_delay_secs: Some(60),
        retry_backoff_factor: Some(1.5),
        cursor_lag_floor_secs: Some(120),
        tls_ca_cert: Some("/etc/pki/ca.pem".into()),
        tls_client_cert: Some("/etc/pki/client.pem".into()),
        tls_client_key_file: Some("/etc/pki/client.key".into()),
        tls_danger_accept_invalid_certs: Some(false),
    }
}

/// **The one this feature keeps regressing on.** An absent column means
/// "airway's compiled-in default", so an all-NULL row must leave every
/// `HttpConfig` / `RetryConfig` value exactly where airway put it — not stamp
/// zeros, not stamp an oxy-chosen constant.
///
/// Asserted against `builtin()` rather than against literals, so the day
/// airway changes a built-in this test still says the right thing instead of
/// pinning a number oxy has no business knowing.
#[test]
fn an_absent_row_leaves_every_airway_default_alone() {
    let global = DeploymentValues::default()
        .to_global()
        .expect("an empty row is valid");

    assert!(global.timeout.is_none());
    assert!(global.max_retries.is_none());
    assert!(global.user_agent.is_none());
    assert!(global.retry_initial_delay.is_none());
    assert!(global.retry_max_delay.is_none());
    assert!(global.retry_backoff_factor.is_none());
    // The one that must not become `Some(ZERO)`: a zero floor is a value
    // airway refuses, so collapsing absence into it would make an unconfigured
    // deployment fail to install at all.
    assert!(global.cursor_lag_floor.is_none());
    assert!(global.tls.is_none());

    // And, driven through the gap-fillers, the compiled-in values survive.
    let http = global.apply_to_http(HttpConfig::builtin());
    assert_eq!(http.timeout, HttpConfig::builtin().timeout);
    assert_eq!(http.max_retries, HttpConfig::builtin().max_retries);
    assert_eq!(http.retry_delay, HttpConfig::builtin().retry_delay);
    assert!(http.user_agent.is_none());
    assert_ne!(
        http.timeout,
        Duration::ZERO,
        "absence collapsed to zero — the exact regression this test exists for"
    );

    let retry = global.apply_to_retry(RetryConfig::builtin());
    assert_eq!(retry.max_retries, RetryConfig::builtin().max_retries);
    assert_eq!(retry.initial_delay, RetryConfig::builtin().initial_delay);
    assert_eq!(retry.max_delay, RetryConfig::builtin().max_delay);
    assert_eq!(retry.backoff_factor, RetryConfig::builtin().backoff_factor);
}

/// Absence is not zero at the *round-trip* level either: reading the defaults
/// back out must produce `None`s again, never `Some(0)`.
#[test]
fn absence_survives_the_round_trip_as_absence() {
    let effective = DeploymentValues::default()
        .effective()
        .expect("an empty row is valid");
    assert_eq!(
        effective,
        DeploymentValues::default(),
        "an unset setting came back as a value"
    );
}

/// Each of the eight settings reaches the airway field it names, in the unit
/// its column name states.
#[test]
fn every_setting_reaches_its_airway_field_in_the_stated_unit() {
    let global = fully_configured().to_global().expect("well-formed values");

    assert_eq!(global.timeout, Some(Duration::from_secs(90)), "seconds");
    assert_eq!(global.max_retries, Some(7));
    assert_eq!(global.user_agent.as_deref(), Some("oxy-airway/1.0"));
    assert_eq!(
        global.retry_initial_delay,
        Some(Duration::from_millis(250)),
        "milliseconds, per the `_ms` suffix the column carries"
    );
    assert_eq!(global.retry_max_delay, Some(Duration::from_secs(60)));
    assert_eq!(global.retry_backoff_factor, Some(1.5));
    assert_eq!(
        global.cursor_lag_floor,
        Some(Duration::from_secs(120)),
        "seconds, per the `_secs` suffix the column carries"
    );

    let tls = global.tls.clone().expect("tls columns were set");
    assert_eq!(tls.ca_cert.as_deref(), Some("/etc/pki/ca.pem"));
    assert_eq!(tls.client_cert.as_deref(), Some("/etc/pki/client.pem"));
    assert_eq!(tls.client_key_file.as_deref(), Some("/etc/pki/client.key"));
    assert!(!tls.danger_accept_invalid_certs);

    // And the whole thing reads back identical — the property the drift
    // comparison rests on.
    assert_eq!(
        DeploymentValues::from_global(&global),
        fully_configured(),
        "a configured row must survive the airway round trip unchanged"
    );
}

/// [`COLUMNS`] is pinned to what the lookup actually answers, in both
/// directions, and the only keys airway asks for that we decline are the two
/// policy-tier ones.
///
/// Recorded from `from_lookup` itself rather than transcribed, the same way
/// airway pins its own roster: a column nothing reads is a knob that does
/// nothing, and a key read but absent from `COLUMNS` never gets selected out
/// of the table.
#[test]
fn the_column_roster_is_exactly_what_the_lookup_answers() {
    let asked = RefCell::new(Vec::new());
    let full = fully_configured();
    GlobalConfig::from_lookup(&|key| {
        asked.borrow_mut().push(key.to_string());
        full.lookup(key)
    })
    .expect("well-formed values");

    let mut asked = asked.into_inner();
    asked.sort();
    asked.dedup();

    let answered: Vec<String> = asked
        .iter()
        .filter(|k| full.lookup(k).is_some())
        .cloned()
        .collect();
    let mut declared: Vec<String> = COLUMNS.iter().map(|c| (*c).to_string()).collect();
    declared.sort();
    assert_eq!(
        answered, declared,
        "`COLUMNS` and the keys the lookup answers have diverged"
    );

    let declined: Vec<&String> = asked.iter().filter(|k| full.lookup(k).is_none()).collect();
    assert_eq!(
        declined,
        vec!["contract_policy", "environment"],
        "the operational tier must decline exactly the two policy-tier keys — a new \
         declined key is a setting airway offers and this table silently drops"
    );
}

/// A bad value is an error naming the key, not a silent fall-back to the
/// default — airway's rule, reached through our mapping. Also proves the
/// validation really is airway's: none of these rules are written on the oxy
/// side.
#[test]
fn a_bad_value_is_an_error_that_names_the_key() {
    let cases: Vec<(&str, DeploymentValues)> = vec![
        (
            "timeout_secs",
            DeploymentValues {
                timeout_secs: Some(0),
                ..Default::default()
            },
        ),
        (
            "retry_max_delay_secs",
            DeploymentValues {
                retry_max_delay_secs: Some(0),
                ..Default::default()
            },
        ),
        (
            "retry_initial_delay_ms",
            DeploymentValues {
                retry_initial_delay_ms: Some(0),
                ..Default::default()
            },
        ),
        (
            "retry_backoff_factor",
            DeploymentValues {
                retry_backoff_factor: Some(0.5),
                ..Default::default()
            },
        ),
        // A zero floor is refused, not read as "no floor". `max(lag, 0)` is
        // `lag` for every resource, so accepting it would store a deployment
        // position that raises nothing — the one value indistinguishable from
        // omitting the key, and therefore the one that must not be spelled the
        // same way. `None` is how you say "no floor"; see the `absent` case
        // below it.
        (
            "cursor_lag_floor_secs",
            DeploymentValues {
                cursor_lag_floor_secs: Some(0),
                ..Default::default()
            },
        ),
        (
            "user_agent",
            DeploymentValues {
                user_agent: Some(String::new()),
                ..Default::default()
            },
        ),
        (
            "tls_client_cert",
            DeploymentValues {
                tls_client_cert: Some("/etc/pki/client.pem".into()),
                ..Default::default()
            },
        ),
    ];
    for (key, values) in cases {
        let err = values
            .to_global()
            .expect_err("a refused value must not fall back to airway's default")
            .to_string();
        assert!(err.contains(key), "the key must be named, got: {err}");
    }
}

/// The other half of the zero rule above, and the reason airway *refuses* a
/// zero floor rather than coercing it: **absence is no floor**, and absence has
/// to stay reachable and valid. If a zero were quietly normalised to `None` the
/// two spellings would mean the same thing, which is the ambiguity the refusal
/// exists to prevent — and if absence were normalised to `Some(ZERO)` an
/// unconfigured deployment would fail to install at all.
#[test]
fn an_absent_cursor_lag_floor_means_no_floor_and_stays_valid() {
    let global = DeploymentValues::default()
        .to_global()
        .expect("no floor is a valid deployment");
    assert_eq!(global.cursor_lag_floor, None, "absence became a floor");
    assert_eq!(
        DeploymentValues::from_global(&global).cursor_lag_floor_secs,
        None,
        "absence came back as a value on the round trip"
    );
}

/// A floor airway only *warns* about is still **accepted** here. Pinned
/// because the obvious over-correction to the zero rule is an oxy-side range
/// check, and a ceiling is the one direction this key deliberately is not:
/// capping a lag a vendor genuinely needs reintroduces the skip the
/// declaration exists to prevent. Upstream owns the caution; oxy owns no copy
/// of it, so 60 days must save.
#[test]
fn an_implausibly_large_floor_is_warned_about_upstream_not_refused_here() {
    let sixty_days = 60 * 24 * 60 * 60;
    let global = DeploymentValues {
        cursor_lag_floor_secs: Some(sixty_days),
        ..Default::default()
    }
    .to_global()
    .expect("a large floor is a warning upstream, never a refusal");
    assert_eq!(
        global.cursor_lag_floor,
        Some(Duration::from_secs(sixty_days))
    );
}

/// Zero attempts is the one coherent zero — "do not retry" — and it must stay
/// reachable. Pinned because the fix for "absence is not zero" is tempting to
/// over-apply into "zero is never allowed".
#[test]
fn zero_max_retries_is_a_choice_and_stays_allowed() {
    let global = DeploymentValues {
        max_retries: Some(0),
        ..Default::default()
    }
    .to_global()
    .expect("zero attempts is a coherent request");
    assert_eq!(global.max_retries, Some(0));
    assert_eq!(global.apply_to_http(HttpConfig::builtin()).max_retries, 0);
}

/// Nothing configured on either side is **agreement**, not "unknown". Both
/// mean airway's built-ins, which is a definite state.
#[test]
fn two_empty_sides_do_not_drift() {
    assert!(drift(&DeploymentValues::default(), &DeploymentValues::default()).is_empty());
}

#[test]
fn identical_configured_and_installed_do_not_drift() {
    let v = fully_configured();
    assert!(drift(&v, &v).is_empty());
}

/// A `Some` against a `None` is drift in **both** directions: one side took
/// the built-in and the other did not, and an operator needs to see that
/// whichever way round it happened.
#[test]
fn a_set_value_against_an_unset_one_drifts_either_way() {
    let configured = DeploymentValues {
        timeout_secs: Some(90),
        ..Default::default()
    };
    assert_eq!(
        drift(&configured, &DeploymentValues::default()),
        vec!["timeout_secs"]
    );
    assert_eq!(
        drift(&DeploymentValues::default(), &configured),
        vec!["timeout_secs"],
        "clearing a setting the process still has installed is drift too"
    );
}

/// Every field participates, and the report names each drifted setting rather
/// than collapsing to a boolean.
#[test]
fn every_setting_is_compared_and_named() {
    let drifted = drift(&fully_configured(), &DeploymentValues::default());
    assert_eq!(
        drifted, COLUMNS,
        "every column must be compared, in roster order"
    );
}

#[test]
fn a_changed_value_drifts_and_its_siblings_do_not() {
    let mut installed = fully_configured();
    installed.retry_backoff_factor = Some(2.0);
    assert_eq!(
        drift(&fully_configured(), &installed),
        vec!["retry_backoff_factor"]
    );
}

/// **Phantom drift, closed.** `tls_danger_accept_invalid_certs = false` on its
/// own is a written column that configures nothing — airway decides the trust
/// store from what the values *say*, so it yields `tls: None`. Comparing the
/// raw row against an installed `GlobalConfig` would report drift forever;
/// comparing `effective()` forms does not.
#[test]
fn a_written_but_inert_tls_flag_is_not_drift() {
    let row = DeploymentValues {
        tls_danger_accept_invalid_certs: Some(false),
        ..Default::default()
    };
    let effective = row.effective().expect("valid");
    assert_eq!(
        effective.tls_danger_accept_invalid_certs, None,
        "airway does not treat a bare `false` as a configured trust store"
    );
    assert!(
        drift(&effective, &DeploymentValues::default()).is_empty(),
        "an inert flag reported drift against a process running airway's defaults"
    );
    // The raw form is what would have lied — kept as the premise, so this
    // test can't pass by `effective()` becoming a no-op.
    assert!(!drift(&row, &DeploymentValues::default()).is_empty());
}

/// The other half of the same normalisation: a real TLS setting makes the
/// `danger` flag materialise as `Some(false)` on both sides, so it does not
/// drift either.
#[test]
fn a_real_tls_setting_normalises_the_danger_flag_on_both_sides() {
    let row = DeploymentValues {
        tls_ca_cert: Some("/etc/pki/ca.pem".into()),
        ..Default::default()
    };
    let effective = row.effective().expect("valid");
    assert_eq!(effective.tls_danger_accept_invalid_certs, Some(false));
    assert!(drift(&effective, &effective).is_empty());
}

/// `installed_values()` reports what was installed — and this is the **only**
/// test in the crate that installs, because airway's `install` is a
/// process-wide `OnceLock`. nextest runs each test in its own process, which
/// is what makes that affordable; under `cargo test` (one process, which this
/// repo does not use) a second installer would make one of them meaningless.
#[test]
fn installed_values_report_what_this_process_installed() {
    assert!(
        installed_values().is_none(),
        "nothing may have installed before this test — see the doc comment"
    );
    let values = fully_configured();
    global::install(values.to_global().expect("valid")).expect("first install in this process");

    assert_eq!(
        installed_values(),
        Some(values),
        "the installed tier must read back as the values that were installed"
    );
}
