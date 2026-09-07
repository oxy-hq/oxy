//! Every `BaselineOutcome` a values read can return, and what the run does
//! about it.
//!
//! Asserted here rather than through `FitProbe::probe` because the thing worth
//! pinning is the *decision*, not the query: the bug this file exists for was
//! a discarded outcome, and a test that needed a warehouse to catch it would
//! never have run in CI. `classify_values` is pure, so every variant —
//! including the ones no executor emits today — is one line.

use oxy_airlayer_compat::engine::metric_tree_ops::BaselineOutcome as O;

use super::{ValuesVerdict, classify_values};

/// The ordinary case, and the only one that is silent.
#[test]
fn a_clean_read_is_complete() {
    assert_eq!(
        classify_values(&O::Valued {
            unreadable: Vec::new()
        }),
        ValuesVerdict::Complete,
    );
}

/// The regression this module was written for.
///
/// Before the fix the outcome was dropped, so an executor error left `values`
/// empty, every `EdgeFit` unlevelled, and `Outcome::classify` scored the
/// period `Refused` with `refusal` NULL — a broken read wearing the label the
/// taxonomy reserves for the model honestly declining. It must fail the run.
#[test]
fn an_executor_error_fails_the_run_rather_than_refusing() {
    let verdict = classify_values(&O::ExecutorError("connection reset".to_string()));

    match verdict {
        ValuesVerdict::Fatal(what) => assert!(
            what.contains("connection reset"),
            "the fatal message must carry the warehouse's own error, got: {what}",
        ),
        other => panic!("an executor error must be fatal, got {other:?}"),
    }
}

/// A partial read still measures something, so it warns rather than failing —
/// but it must not be mistaken for a clean one, or the warn never fires.
#[test]
fn a_partial_read_is_degraded_and_names_the_measures() {
    let verdict = classify_values(&O::Valued {
        unreadable: vec!["store_day.net_sales".to_string()],
    });

    match verdict {
        ValuesVerdict::Degraded(what) => assert!(
            what.contains("store_day.net_sales"),
            "a degraded read must name what was lost, got: {what}",
        ),
        other => panic!("a partial read must be degraded, got {other:?}"),
    }
}

/// An empty window is a legitimate state for an early period, so it degrades
/// rather than failing — but it is still not `Complete`.
#[test]
fn the_non_fatal_empty_shapes_all_degrade() {
    for outcome in [
        O::NoRows,
        O::NoMatchingColumns,
        O::NothingRequested,
        O::UnreadableValues(vec!["store_day.marketing_spend".to_string()]),
    ] {
        assert!(
            matches!(classify_values(&outcome), ValuesVerdict::Degraded(_)),
            "{outcome:?} must degrade, not pass silently or fail the run",
        );
    }
}

/// `NoRows` and `NothingRequested` are different diagnoses — an empty window
/// versus an empty tree — and must not collapse into one message, or the warn
/// sends someone to widen a window that was never the problem.
#[test]
fn an_empty_window_and_an_empty_tree_read_differently() {
    let (ValuesVerdict::Degraded(no_rows), ValuesVerdict::Degraded(nothing)) = (
        classify_values(&O::NoRows),
        classify_values(&O::NothingRequested),
    ) else {
        panic!("both must be degraded");
    };

    assert_ne!(
        no_rows, nothing,
        "an empty window and an unreachable root must not share a message",
    );
}
