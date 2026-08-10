//! The two per-run admission policies, resolved from wire strings.
//!
//! [`airway::connector::ContractPolicy`] and [`airway::connector::Environment`]
//! implement `FromStr` but **not** `Serialize`/`Deserialize`, so they cannot
//! ride the durable queue payload as themselves. Oxy carries them as strings
//! on `TaskSpec::Airway` and parses here — one place, so the diagnostic
//! wording is not re-invented per call site.
//!
//! Both default to today's behaviour (`permissive` / `production`), so a run
//! that says nothing behaves exactly as it did before 0.1.23.

use airway::connector::{ContractPolicy, Environment};

use crate::error::AirwayError;

/// The contract policy and vendor environment a single run is admitted under.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AirwayAdmission {
    pub contract_policy: ContractPolicy,
    pub environment: Environment,
}

impl AirwayAdmission {
    /// Parse both policies from their wire spellings.
    ///
    /// `None` means "unset" and takes the airway default, which is today's
    /// behaviour. An unrecognised spelling is an **error**, never a silent
    /// fall-back to the default: a typo that degrades `require_declared` to
    /// `permissive` is the tightened policy quietly not applying, which is
    /// indistinguishable in the data from a deployment that never set it.
    pub fn from_strings(
        policy: Option<&str>,
        environment: Option<&str>,
    ) -> Result<Self, AirwayError> {
        let contract_policy = match policy {
            None => ContractPolicy::default(),
            Some(s) => s.parse::<ContractPolicy>().map_err(|()| {
                AirwayError::Other(format!(
                    "unknown airway `contract_policy` `{s}` \
                     (expected `permissive`, `require_declared`, or `forbid_opaque`)"
                ))
            })?,
        };
        let environment = match environment {
            None => Environment::default(),
            Some(s) => s.parse::<Environment>().map_err(|()| {
                AirwayError::Other(format!(
                    "unknown airway `environment` `{s}` (expected `production` or `sandbox`)"
                ))
            })?,
        };
        Ok(Self {
            contract_policy,
            environment,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_strings_are_the_permissive_production_defaults() {
        let a = AirwayAdmission::from_strings(None, None).expect("defaults parse");
        assert_eq!(a.contract_policy, ContractPolicy::Permissive);
        assert_eq!(a.environment, Environment::Production);
        assert_eq!(a, AirwayAdmission::default());
    }

    #[test]
    fn parses_every_accepted_spelling() {
        for (s, expected) in [
            ("permissive", ContractPolicy::Permissive),
            ("require_declared", ContractPolicy::RequireDeclared),
            ("forbid_opaque", ContractPolicy::ForbidOpaque),
        ] {
            let a = AirwayAdmission::from_strings(Some(s), None).expect(s);
            assert_eq!(a.contract_policy, expected, "spelling `{s}`");
        }
        for (s, expected) in [
            ("production", Environment::Production),
            ("sandbox", Environment::Sandbox),
        ] {
            let a = AirwayAdmission::from_strings(None, Some(s)).expect(s);
            assert_eq!(a.environment, expected, "spelling `{s}`");
        }
    }

    /// An unknown spelling must fail loudly rather than silently taking the
    /// default. A typo'd `require-declared` degrading to `permissive` is the
    /// tightened policy quietly not applying — the failure this whole stage
    /// exists to make impossible.
    #[test]
    fn unknown_policy_is_an_error_naming_the_alternatives() {
        let err = AirwayAdmission::from_strings(Some("require-declared"), None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("require-declared"), "got: {msg}");
        assert!(
            msg.contains("require_declared"),
            "must name the valid spellings: {msg}"
        );
    }

    #[test]
    fn unknown_environment_is_an_error_naming_the_alternatives() {
        let err = AirwayAdmission::from_strings(None, Some("staging")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("staging"), "got: {msg}");
        assert!(
            msg.contains("sandbox"),
            "must name the valid spellings: {msg}"
        );
    }
}
