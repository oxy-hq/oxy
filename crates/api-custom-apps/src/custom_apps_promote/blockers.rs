//! What refuses an act, and what only warns — decided from facts the plan has
//! already read, with no database in sight (environments design §5.5a,
//! ruling 9).
//!
//! | Blocker | When |
//! | --- | --- |
//! | `staging_never_served` | production moves to a build staging never served (invariant 1) |
//! | `validation_not_passed` | production moves to a build whose validation is not `passed` |
//! | `required_secrets_unset` | a production **promote** to a build declaring a `required` key production has no value for. A rollback, the emergency path, only warns |
//! | `schedule_removal_unconfirmed` | production's move would retire a registered schedule the request did not name in `confirm_schedule_removals` — a rollback too |

use super::error::{
    Blocker, REQUIRED_SECRETS_UNSET, SCHEDULE_REMOVAL_UNCONFIRMED, STAGING_NEVER_SERVED,
    VALIDATION_NOT_PASSED,
};

use crate::custom_apps_build_state::{PromoteRefusal, Validation};

/// Which verb is planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Promote,
    Rollback,
}

impl Act {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::Rollback => "rollback",
        }
    }
}

/// What a production move's checks read. A staging move has none of these:
/// it is a pointer move that never blocks.
#[derive(Debug, Clone, Copy)]
pub struct ProductionFacts<'a> {
    pub act: Act,
    /// The build's publish id, for the sentences.
    pub build_label: &'a str,
    pub staging_served: bool,
    pub validation: &'a Validation,
    pub required_unset: &'a [String],
    /// Registered schedules the move would retire, by function.
    pub schedule_removals: &'a [String],
    /// `confirm_schedule_removals`, as the request named them.
    pub confirmed_removals: &'a [String],
}

/// The blockers of a production move, in the order of the table above, and
/// the warnings that do not block.
pub fn evaluate(facts: ProductionFacts<'_>) -> (Vec<Blocker>, Vec<String>) {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if !facts.staging_served {
        blockers.push(staging_never_served(facts.build_label));
    }
    if let Err(PromoteRefusal::ValidationNotPassed { status }) = facts.validation.check() {
        blockers.push(validation_not_passed(facts.build_label, &status));
    }
    if !facts.required_unset.is_empty() {
        let keys = facts.required_unset.join(", ");
        match facts.act {
            Act::Promote => blockers.push(Blocker {
                code: REQUIRED_SECRETS_UNSET,
                message: format!(
                    "the build declares required keys production has no value for: {keys} — \
                     set them first (`oxyc env secret` or the console's Secrets tab)"
                ),
            }),
            Act::Rollback => warnings.push(format!(
                "the target declares required keys production has no value for: {keys}"
            )),
        }
    }
    let unconfirmed: Vec<&str> = facts
        .schedule_removals
        .iter()
        .filter(|name| !facts.confirmed_removals.contains(*name))
        .map(String::as_str)
        .collect();
    if !unconfirmed.is_empty() {
        blockers.push(Blocker {
            code: SCHEDULE_REMOVAL_UNCONFIRMED,
            message: format!(
                "this retires the schedule of {}: name each to confirm \
                 (`--remove-schedule <function>`, or `confirm_schedule_removals`)",
                unconfirmed.join(", ")
            ),
        });
    }
    (blockers, warnings)
}

/// Invariant 1: production only ever serves a build staging has served.
pub fn staging_never_served(build_label: &str) -> Blocker {
    Blocker {
        code: STAGING_NEVER_SERVED,
        message: format!(
            "staging has never served build {build_label:?}, and production only serves a build \
             staging has served — publish it to staging (or promote it there from its sandbox) \
             first"
        ),
    }
}

pub fn validation_not_passed(build_label: &str, status: &str) -> Blocker {
    Blocker {
        code: VALIDATION_NOT_PASSED,
        message: format!(
            "build {build_label:?} has validation status {status:?}, not \"passed\" — it cannot \
             be made live"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(act: Act) -> ProductionFacts<'a> {
        ProductionFacts {
            act,
            build_label: "b-7",
            staging_served: true,
            validation: &Validation::Passed,
            required_unset: &[],
            schedule_removals: &[],
            confirmed_removals: &[],
        }
    }

    fn codes(blockers: &[Blocker]) -> Vec<&'static str> {
        blockers.iter().map(|b| b.code).collect()
    }

    #[test]
    fn a_build_staging_served_with_nothing_unset_has_no_blocker() {
        let (blockers, warnings) = evaluate(facts(Act::Promote));
        assert!(blockers.is_empty() && warnings.is_empty());
    }

    #[test]
    fn invariant_one_and_validation_block_both_verbs() {
        for act in [Act::Promote, Act::Rollback] {
            let (blockers, _) = evaluate(ProductionFacts {
                staging_served: false,
                validation: &Validation::of("failed"),
                ..facts(act)
            });
            assert_eq!(
                codes(&blockers),
                vec![STAGING_NEVER_SERVED, VALIDATION_NOT_PASSED],
                "{act:?}"
            );
        }
    }

    /// An unset required key blocks a promote; a rollback, the emergency
    /// path, goes ahead and says so.
    #[test]
    fn an_unset_required_key_blocks_a_promote_and_warns_a_rollback() {
        let unset = ["STRIPE_KEY".to_string()];
        let (blockers, warnings) = evaluate(ProductionFacts {
            required_unset: &unset,
            ..facts(Act::Promote)
        });
        assert_eq!(codes(&blockers), vec![REQUIRED_SECRETS_UNSET]);
        assert!(warnings.is_empty());
        let (blockers, warnings) = evaluate(ProductionFacts {
            required_unset: &unset,
            ..facts(Act::Rollback)
        });
        assert!(blockers.is_empty());
        assert!(warnings[0].contains("STRIPE_KEY"), "{warnings:?}");
    }

    /// Every removed schedule must be named; naming one of two is not enough,
    /// for either verb.
    #[test]
    fn each_removed_schedule_must_be_confirmed_by_name() {
        let removals = ["nightly".to_string(), "hourly".to_string()];
        let partly = ["nightly".to_string()];
        for act in [Act::Promote, Act::Rollback] {
            let (blockers, _) = evaluate(ProductionFacts {
                schedule_removals: &removals,
                confirmed_removals: &partly,
                ..facts(act)
            });
            assert_eq!(codes(&blockers), vec![SCHEDULE_REMOVAL_UNCONFIRMED]);
            assert!(blockers[0].message.contains("hourly"));
            assert!(!blockers[0].message.contains("nightly"));
            let (blockers, _) = evaluate(ProductionFacts {
                schedule_removals: &removals,
                confirmed_removals: &removals,
                ..facts(act)
            });
            assert!(blockers.is_empty(), "{act:?}");
        }
    }
}
