//! `error.type` for a failed host op, decided on the [`HostError`]'s kind
//! where the kind says, and on its text everywhere else.
//!
//! The text route is `host_call_attrs::classify_host_error_for`, unchanged:
//! it reads the host's own phrasing, and its markers are the reason that text
//! is a contract. This is the route in front of it for the kinds the broker
//! knows without reading a word, so a refusal reworded later keeps its kind.
//! Both routes must agree on every message the host writes; a test below
//! holds that for every capability gate and for the environment's labels.

use super::host_call_attrs::classify_host_error_for;
use super::host_error::{HostError, HostErrorKind};

/// The kind-first classification of a failed host call.
///
/// `environment_labels` is as in `classify_host_error_for`: only where the
/// environment policy decides is a held or refused call the policy's own, and
/// anywhere else the kind means nothing and the text classifies it, so a
/// production failure always pages.
pub(super) fn classify_host_error_typed(
    error: &HostError,
    environment_labels: bool,
) -> &'static str {
    match error.kind {
        HostErrorKind::Held if environment_labels => "held",
        HostErrorKind::Refused if environment_labels => "refused",
        HostErrorKind::CapabilityMissing { .. } => "not_allowed",
        _ => classify_host_error_for(&error.message, environment_labels),
    }
}

#[cfg(test)]
mod tests {
    use super::super::env_policy::{EnvPolicy, HostOp};
    use super::super::host::{FunctionCapabilities, WriterCapability};
    use super::*;

    /// The typed route and the text route give one answer for each message.
    fn agree(error: &HostError, expected: &str) {
        for labels in [false, true] {
            let text = classify_host_error_for(&error.message, labels);
            let typed = classify_host_error_typed(error, labels);
            assert_eq!(typed, text, "{error} (labels: {labels})");
        }
        assert_eq!(classify_host_error_typed(error, true), expected, "{error}");
    }

    #[test]
    fn every_capability_gate_the_host_writes_classifies_as_the_text_does() {
        let nothing = FunctionCapabilities::default();
        let mut seen = 0;
        for op in HostOp::ALL {
            if let Err(error) = nothing.refuse(*op) {
                assert!(
                    matches!(error.kind, HostErrorKind::CapabilityMissing { .. }),
                    "{op:?}: {error}"
                );
                agree(&error, "not_allowed");
                seen += 1;
            }
        }
        assert!(seen > 10, "only {seen} gated ops were refused");
    }

    #[test]
    fn a_slug_that_backs_no_schema_classifies_as_the_text_does() {
        let caps = FunctionCapabilities {
            oltp: WriterCapability::resolve(true, "9lives"),
            ..Default::default()
        };
        let error = caps.refuse(HostOp::OltpExec).unwrap_err();
        assert_eq!(error.kind, HostErrorKind::Backend);
        for labels in [false, true] {
            assert_eq!(
                classify_host_error_typed(&error, labels),
                classify_host_error_for(&error.message, labels)
            );
        }
    }

    #[test]
    fn held_and_refused_are_the_environments_only_where_it_decides() {
        let held = HostError::held("HeldInStaging: ctx.fetch was not performed.");
        let refused = HostError::refused("EnvironmentRefused: ctx.fetch is refused.");
        assert_eq!(classify_host_error_typed(&held, true), "held");
        assert_eq!(classify_host_error_typed(&refused, true), "refused");
        for error in [&held, &refused] {
            assert_eq!(
                classify_host_error_typed(error, false),
                classify_host_error_for(&error.message, false),
                "off the environment's decision the text classifies it"
            );
        }
    }

    #[test]
    fn the_policys_own_words_carry_the_kind_their_label_names() {
        let policy = EnvPolicy::production();
        for op in HostOp::ALL {
            let held = policy.held_error(*op);
            assert_eq!(held.kind, HostErrorKind::Held, "{op:?}");
            assert_eq!(HostError::from(held.message.clone()).kind, held.kind);
            let refused = policy.refusal(*op, "a fix");
            assert_eq!(refused.kind, HostErrorKind::Refused, "{op:?}");
            assert_eq!(HostError::from(refused.message.clone()).kind, refused.kind);
        }
    }

    #[test]
    fn a_backend_failure_is_classified_by_its_text() {
        for text in [
            "warehouse insert failed: permission denied",
            "unknown op 'rename'",
            "function timed out",
            "boom",
        ] {
            let error = HostError::backend(text);
            assert_eq!(
                classify_host_error_typed(&error, true),
                classify_host_error_for(text, true)
            );
        }
    }
}
