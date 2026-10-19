//! What an `app_builds` row says about whether the build may go live, as one
//! typed value instead of string compares scattered over every gate.
//!
//! Three columns decide it. `validation_status` is the validator's verdict
//! (`custom_apps_validate`: gate 1 stamps `passed` at publish; a deploy-time
//! probe may one day downgrade it). `published_token_id` is the mark of a
//! build a **sandbox agent token** published (`custom_apps_agent_built`):
//! nobody approved it, so no promote, rollback or production fallback may
//! ship it. `published_to` names the environment the publish moved first and
//! is carried for the refusal's message.
//!
//! [`BuildState::promotable`] is the one answer a production move asks
//! (`custom_apps_promote`), and the read side derives the agent mark through
//! [`Provenance`] too, so the write-side refusal and the serve-side fallback
//! cannot read the columns differently.
//!
//! The refusals carry a code and the facts they were made from; each caller
//! answers in the shape its route always has (`custom_apps_promote::error`
//! for a promote, `PublishError` for a publish).

use entity::app_builds;
use uuid::Uuid;

/// The code a build whose verdict is not `passed` is refused with.
pub const VALIDATION_NOT_PASSED: &str = "validation_not_passed";

/// The validator's recorded verdict for a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validation {
    /// `validation_status = 'passed'`: the only verdict a build goes live on.
    Passed,
    /// Anything else, verbatim — `pending`, `failed`, or a value nothing
    /// writes today — so the refusal can name it.
    Other(String),
}

impl Validation {
    pub fn of(status: &str) -> Self {
        if status == "passed" {
            Self::Passed
        } else {
            Self::Other(status.to_string())
        }
    }

    /// `Err` naming the verdict unless it is `passed`.
    pub fn check(&self) -> Result<(), PromoteRefusal> {
        match self {
            Self::Passed => Ok(()),
            Self::Other(status) => Err(PromoteRefusal::ValidationNotPassed {
                status: status.clone(),
            }),
        }
    }
}

/// Who published a build, as far as promotion cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// A person, a CI job or a machine publish: promotable as any build is.
    /// `env` is the environment the publish moved first (`published_to`);
    /// `None` on a row written before that column existed.
    Person { env: Option<String> },
    /// A sandbox agent token (`published_token_id`): never promoted, never
    /// rolled back to, never served by production's fallback.
    AgentBuilt { token_id: Uuid },
}

impl Provenance {
    pub fn of(published_to: Option<&str>, published_token_id: Option<Uuid>) -> Self {
        match published_token_id {
            Some(token_id) => Self::AgentBuilt { token_id },
            None => Self::Person {
                env: published_to.map(str::to_string),
            },
        }
    }

    /// The mark alone, for a reader that selected only `published_token_id`.
    pub fn from_token(published_token_id: Option<Uuid>) -> Self {
        Self::of(None, published_token_id)
    }

    /// The read side's question: does production refuse to fall back to it?
    pub fn is_agent_built(&self) -> bool {
        matches!(self, Self::AgentBuilt { .. })
    }

    /// `Err` naming the token when a sandbox agent token published the build.
    pub fn check(&self) -> Result<(), PromoteRefusal> {
        match self {
            Self::Person { .. } => Ok(()),
            Self::AgentBuilt { token_id } => Err(PromoteRefusal::AgentBuilt {
                token_id: *token_id,
            }),
        }
    }
}

/// Everything a production transition reads off a build row before it moves
/// a pointer to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildState {
    pub provenance: Provenance,
    pub validation: Validation,
}

/// Why a build may not go live. A caller hydrates it into its own answer:
/// the agent-built refusal names the token's minter
/// (`custom_apps_agent_built::AgentBuilt::describe`), the validation one the
/// verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromoteRefusal {
    /// The recorded verdict is not `passed` ([`VALIDATION_NOT_PASSED`]).
    ValidationNotPassed { status: String },
    /// A sandbox agent token published it (`409 draft_published_by_agent`).
    AgentBuilt { token_id: Uuid },
}

impl PromoteRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ValidationNotPassed { .. } => VALIDATION_NOT_PASSED,
            Self::AgentBuilt { .. } => crate::server::api::custom_apps_agent_built::PROMOTE_REFUSED,
        }
    }
}

impl BuildState {
    pub fn of(build: &app_builds::Model) -> Self {
        Self::from_columns(
            build.published_to.as_deref(),
            build.published_token_id,
            &build.validation_status,
        )
    }

    pub fn from_columns(
        published_to: Option<&str>,
        published_token_id: Option<Uuid>,
        validation_status: &str,
    ) -> Self {
        Self {
            provenance: Provenance::of(published_to, published_token_id),
            validation: Validation::of(validation_status),
        }
    }

    /// Whether production may move to this build. Whose the build is is asked
    /// first, as the production move always has: a token's build is refused
    /// as unapproved whatever else is true of it, before its blockers are
    /// listed.
    pub fn promotable(&self) -> Result<(), PromoteRefusal> {
        self.provenance.check()?;
        self.validation.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> Uuid {
        Uuid::from_u128(0x70)
    }

    #[test]
    fn a_passed_build_a_person_published_is_promotable() {
        let state = BuildState::from_columns(Some("staging"), None, "passed");
        assert_eq!(state.promotable(), Ok(()));
        assert_eq!(
            state.provenance,
            Provenance::Person {
                env: Some("staging".into())
            }
        );
        assert!(!state.provenance.is_agent_built());
    }

    /// A legacy row with no `published_to` is a person's build all the same.
    #[test]
    fn a_row_without_an_environment_is_still_a_persons_build() {
        let state = BuildState::from_columns(None, None, "passed");
        assert_eq!(state.promotable(), Ok(()));
        assert_eq!(state.provenance, Provenance::Person { env: None });
    }

    #[test]
    fn a_verdict_other_than_passed_is_refused_naming_the_verdict() {
        for status in ["pending", "failed", ""] {
            let refused = BuildState::from_columns(Some("staging"), None, status)
                .promotable()
                .expect_err(status);
            assert_eq!(
                refused,
                PromoteRefusal::ValidationNotPassed {
                    status: status.into()
                }
            );
            assert_eq!(refused.code(), "validation_not_passed");
        }
    }

    #[test]
    fn a_tokens_build_is_refused_however_it_validated() {
        let refused = BuildState::from_columns(Some("dev-a1"), Some(token()), "passed")
            .promotable()
            .expect_err("refused");
        assert_eq!(refused, PromoteRefusal::AgentBuilt { token_id: token() });
        assert_eq!(refused.code(), "draft_published_by_agent");
    }

    /// Whose build it is is asked first: a token's build that also failed
    /// validation is refused as unapproved, as the production move always did.
    #[test]
    fn the_publisher_is_asked_before_the_validator() {
        let refused = BuildState::from_columns(Some("staging"), Some(token()), "failed")
            .promotable()
            .expect_err("refused");
        assert!(matches!(refused, PromoteRefusal::AgentBuilt { .. }));
        // Each half answers alone for the gate that asks only one question.
        let state = BuildState::from_columns(None, Some(token()), "failed");
        assert!(state.provenance.check().is_err());
        assert!(state.validation.check().is_err());
        let state = BuildState::from_columns(None, Some(token()), "passed");
        assert_eq!(state.validation.check(), Ok(()));
    }

    /// The read side's mark is the same derivation: `published_token_id` set
    /// means agent-built, whatever else the row says.
    #[test]
    fn the_read_side_mark_is_the_token_column_alone() {
        assert!(Provenance::from_token(Some(token())).is_agent_built());
        assert!(Provenance::of(Some("staging"), Some(token())).is_agent_built());
        assert!(!Provenance::from_token(None).is_agent_built());
        assert!(!Provenance::of(Some("staging"), None).is_agent_built());
    }
}
