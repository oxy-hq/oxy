//! Tests for the **pure** half of the policy preview.
//!
//! Everything here runs against a fake contract map — no filesystem, no
//! database, no `OXY_DATABASE_URL`, so nothing in this file can self-skip.
//! That is the point of the seam: if a policy-semantics test needed a
//! workspace on disk, [`verdicts`] would be in the wrong place.
//!
//! Every fixture resource is **cursored**. `ContractPolicy::check` skips
//! resources with no cursor field, so an uncursored fixture would pass every
//! policy vacuously and pin nothing — the same trap `agentic-airway`'s
//! `boxed.rs` documents for its admission fixtures.

use std::collections::HashMap;

use agentic_airway::{
    ContractPolicy, EngineError, Environment, ExtractionResult, ResourceInfo, SourceConnector,
    SourceContract, WriteDisposition, admit_with,
};
use async_trait::async_trait;

use super::super::KNOWN_SOURCE_KINDS;
use super::{ResourceVerdict, verdicts};
// `substitute_secret_vars` lives with the scan half (it exists to make a
// *stored* definition constructible) but is itself pure, so its tests stay in
// this file rather than moving into the DB-backed one where they could
// self-skip.
use super::super::preview_scan::substitute_secret_vars;

/// The kind the un-suffixed helper scores under. Since airway 0.1.24 every
/// kind is on the "fixable" side of the `not_fixable_here` split, so this is
/// now just a representative kind rather than a choice with meaning.
const DEFAULT_KIND: &str = "toast";

fn cursored(name: &str) -> ResourceInfo {
    ResourceInfo {
        name: name.to_string(),
        description: None,
        write_disposition: WriteDisposition::Merge,
        primary_key: Some(vec!["guid".to_string()]),
        cursor_field: Some("modifiedDate".to_string()),
    }
}

fn uncursored(name: &str) -> ResourceInfo {
    ResourceInfo {
        cursor_field: None,
        ..cursored(name)
    }
}

/// The fake connector's resource list: one cursored resource per declared
/// contract, or a single undeclared one when the map is empty — which is
/// exactly the `rest_api` shape (declares nothing, cursors everything).
fn fake_resources(contracts: &HashMap<String, SourceContract>) -> Vec<ResourceInfo> {
    if contracts.is_empty() {
        return vec![cursored("charges")];
    }
    let mut names: Vec<&String> = contracts.keys().collect();
    names.sort();
    names.into_iter().map(|n| cursored(n)).collect()
}

fn verdicts_for_kind(
    kind: &str,
    pipeline_ref: &str,
    contracts: &HashMap<String, SourceContract>,
    policy: ContractPolicy,
) -> Vec<ResourceVerdict> {
    verdicts(
        kind,
        pipeline_ref,
        &fake_resources(contracts),
        contracts,
        policy,
    )
}

fn verdicts_for(
    pipeline_ref: &str,
    contracts: &HashMap<String, SourceContract>,
    policy: ContractPolicy,
) -> Vec<ResourceVerdict> {
    verdicts_for_kind(DEFAULT_KIND, pipeline_ref, contracts, policy)
}

#[test]
fn immutable_resources_pass_every_policy() {
    let contracts = HashMap::from([("orders".to_string(), SourceContract::immutable())]);
    for policy in [
        ContractPolicy::Permissive,
        ContractPolicy::RequireDeclared,
        ContractPolicy::ForbidOpaque,
    ] {
        let v = verdicts_for("toast.airway.yml", &contracts, policy);
        assert!(
            v[0].passes,
            "{policy:?} accepts a declared immutable resource"
        );
    }
}

#[test]
fn opaque_resources_fail_forbid_opaque_only() {
    let contracts = HashMap::from([("things".to_string(), SourceContract::opaque())]);

    assert!(verdicts_for("p.airway.yml", &contracts, ContractPolicy::Permissive)[0].passes);
    let strict = verdicts_for("p.airway.yml", &contracts, ContractPolicy::ForbidOpaque);
    assert!(!strict[0].passes);
    assert!(strict[0].reason.as_ref().unwrap().contains("opaque"));
}

/// **An undeclared resource is fixable for every kind Oxy knows.**
///
/// This inverted at airway 0.1.24. Before it, `rest_api` was flagged
/// `not_fixable_here` because `EndpointConfig` had no `contract` field — the
/// operator was told to wait for upstream. #105 added it, so all ~24
/// REST-backed connectors can now declare, and `toast` / `quickbooks` /
/// `weather` always could. There is no longer a kind for which "undeclared"
/// names an action the operator cannot take.
///
/// Written as a loop over [`KNOWN_SOURCE_KINDS`] rather than one case per
/// kind: a kind added to that list without an upstream way to declare would
/// fail here, which is the only signal that would justify bringing the
/// removed allow-list back.
#[test]
fn an_undeclared_resource_is_fixable_for_every_known_kind() {
    for kind in KNOWN_SOURCE_KINDS {
        let v = verdicts_for_kind(
            kind,
            "p.airway.yml",
            &HashMap::new(),
            ContractPolicy::RequireDeclared,
        );
        assert!(
            !v[0].passes,
            "{kind}: an undeclared cursored resource fails"
        );
        assert!(
            !v[0].not_fixable_here,
            "{kind}: every kind can declare a contract since airway 0.1.24, so an undeclared \
             resource is a real, fixable gap — not an upstream limitation"
        );
    }
}

/// **The fix location differs per kind, and the diagnostic must say which.**
///
/// `rest_api` is config-defined — #105 put the declaration on
/// `EndpointConfig::contract`, which `build_rest_api` reads straight out of
/// `source.config`, so it lives in the pipeline's own `.airway.yml`. The other
/// three implement `SourceConnector::contracts()` in airway's Rust source and
/// have no YAML slot at all.
///
/// One generic sentence is wrong for one of those halves, and wrong in the
/// worst direction: `EndpointConfig` does not `deny_unknown_fields`, so a
/// `contract:` key added to a *toast* pipeline on this advice would be parsed,
/// ignored, and leave the resource failing with no error to explain why.
#[test]
fn the_undeclared_diagnostic_names_the_right_site_per_kind() {
    for kind in KNOWN_SOURCE_KINDS {
        let v = verdicts_for_kind(
            kind,
            "p.airway.yml",
            &HashMap::new(),
            ContractPolicy::RequireDeclared,
        );
        let reason = v[0]
            .reason
            .as_ref()
            .expect("a failing verdict has a reason");

        if *kind == "rest_api" {
            assert!(
                reason.contains("source.config.endpoints") && reason.contains("`contract:`"),
                "rest_api declares in YAML, on the endpoint — got: {reason}"
            );
            assert!(
                !reason.contains("contracts()"),
                "rest_api needs no Rust change; pointing at `contracts()` sends the operator \
                 to a file they cannot edit — got: {reason}"
            );
        } else {
            assert!(
                reason.contains("SourceConnector::contracts()"),
                "{kind} declares in airway's Rust source — got: {reason}"
            );
            assert!(
                !reason.contains("source.config.endpoints"),
                "{kind} has no endpoint slot; `EndpointConfig` ignores unknown keys, so this \
                 advice would fail silently — got: {reason}"
            );
        }
    }
}

/// A *declared* `opaque()` under `forbid_opaque` is refused, but it is not an
/// upstream limitation — the operator's move (pick a policy this kind can
/// meet) is one they make in this very UI. Pins one side of the split, so
/// `not_fixable_here` can't degenerate into "any failure".
#[test]
fn a_declared_opaque_failure_is_not_an_upstream_limitation() {
    let contracts = HashMap::from([("things".to_string(), SourceContract::opaque())]);
    let v = verdicts_for_kind(
        "rest_api",
        "p.airway.yml",
        &contracts,
        ContractPolicy::ForbidOpaque,
    );
    assert!(!v[0].passes);
    assert!(
        !v[0].not_fixable_here,
        "a contract that WAS declared is a checked vendor fact, not an upstream gap"
    );
}

/// An undeclared resource reports `undeclared`, never `opaque`. Airway reaches
/// `Opaque` for it through `unwrap_or_default()`, but the two states are the
/// exact distinction `require_declared` turns on.
#[test]
fn an_undeclared_resource_is_labelled_undeclared_not_opaque() {
    let v = verdicts_for("t.airway.yml", &HashMap::new(), ContractPolicy::Permissive);
    assert_eq!(v[0].mutability, "undeclared");
}

/// Uncursored resources are exempt under every policy: they have no
/// incremental window for a contract to constrain, so requiring one would
/// refuse resources with nothing to declare.
#[test]
fn uncursored_resources_are_exempt_from_every_policy() {
    for policy in [
        ContractPolicy::Permissive,
        ContractPolicy::RequireDeclared,
        ContractPolicy::ForbidOpaque,
    ] {
        let v = verdicts(
            "toast",
            "t.airway.yml",
            &[uncursored("menus")],
            &HashMap::new(),
            policy,
        );
        assert!(
            v[0].passes,
            "{policy:?} must not demand a contract for an uncursored resource"
        );
    }
}

/// A contract declared for a resource that doesn't exist fails a tightened
/// policy — upstream refuses it before scoring anything, so a preview that
/// skipped orphans would render all-clear for a pipeline the policy halts.
#[test]
fn an_orphaned_contract_fails_a_tightened_policy() {
    let contracts = HashMap::from([("typo".to_string(), SourceContract::immutable())]);
    let resources = [cursored("orders")];

    let permissive = verdicts(
        "toast",
        "t.airway.yml",
        &resources,
        &contracts,
        ContractPolicy::Permissive,
    );
    assert!(
        permissive.iter().all(|v| v.passes),
        "permissive only warns about an orphan"
    );

    let strict = verdicts(
        "toast",
        "t.airway.yml",
        &resources,
        &contracts,
        ContractPolicy::RequireDeclared,
    );
    let orphan = strict
        .iter()
        .find(|v| v.resource == "typo")
        .expect("the orphan is reported, not dropped");
    assert!(!orphan.passes);
    assert!(
        orphan.not_fixable_here,
        "an orphan is a connector-source typo; no Oxy setting reaches it"
    );
}

// ---------------------------------------------------------------------------
// Differential: the pure verdict vs. airway's own admission
// ---------------------------------------------------------------------------

/// A connector that answers exactly the fixture's resources and contracts, so
/// the real `admit_with` can be asked the same question [`verdicts`] was.
struct FakeConnector {
    resources: Vec<ResourceInfo>,
    contracts: HashMap<String, SourceContract>,
}

#[async_trait]
impl SourceConnector for FakeConnector {
    fn name(&self) -> &str {
        "fake"
    }

    fn resources(&self) -> Vec<ResourceInfo> {
        self.resources.clone()
    }

    fn contracts(&self) -> HashMap<String, SourceContract> {
        self.contracts.clone()
    }

    async fn extract(
        &self,
        _resource: &str,
        _state: Option<&serde_json::Value>,
    ) -> Result<ExtractionResult, EngineError> {
        unimplemented!("admission never extracts")
    }
}

/// [`verdicts`] restates `ContractPolicy::check` per resource; this pins that
/// the restatement agrees with the original on the only thing both answer —
/// does this connector run at all. Without it, an airway bump that changes an
/// admission rule leaves the preview confidently reporting the old one, which
/// is the exact silent-wrongness class `boxed.rs` exists to prevent.
#[test]
fn verdicts_agree_with_airway_admission() {
    let cases: Vec<(&str, Vec<ResourceInfo>, HashMap<String, SourceContract>)> = vec![
        (
            "declared immutable",
            vec![cursored("orders")],
            HashMap::from([("orders".to_string(), SourceContract::immutable())]),
        ),
        (
            "declared versioned",
            vec![cursored("orders")],
            HashMap::from([(
                "orders".to_string(),
                SourceContract::versioned("modifiedDate"),
            )]),
        ),
        (
            "declared opaque",
            vec![cursored("things")],
            HashMap::from([("things".to_string(), SourceContract::opaque())]),
        ),
        (
            "undeclared cursored",
            vec![cursored("charges")],
            HashMap::new(),
        ),
        (
            "undeclared uncursored",
            vec![uncursored("menus")],
            HashMap::new(),
        ),
        (
            "orphaned declaration",
            vec![cursored("orders")],
            HashMap::from([("typo".to_string(), SourceContract::immutable())]),
        ),
        (
            "one good, one undeclared",
            vec![cursored("orders"), cursored("checks")],
            HashMap::from([("orders".to_string(), SourceContract::immutable())]),
        ),
    ];

    for (label, resources, contracts) in cases {
        for policy in [
            ContractPolicy::Permissive,
            ContractPolicy::RequireDeclared,
            ContractPolicy::ForbidOpaque,
        ] {
            let connector = FakeConnector {
                resources: resources.clone(),
                contracts: contracts.clone(),
            };
            let admitted = admit_with(&connector, policy, Environment::Production).is_ok();
            let previewed = verdicts("toast", "t.airway.yml", &resources, &contracts, policy)
                .iter()
                .all(|v| v.passes);
            assert_eq!(
                previewed, admitted,
                "`{label}` under {policy:?}: the preview and airway's own admission disagree"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Credential placeholders
// ---------------------------------------------------------------------------

/// Flat and nested `*_var` references both become literals, so a connector
/// whose `Params` is `deny_unknown_fields` around a required credential still
/// constructs. Without this the toast/quickbooks arms would reject every real
/// pipeline and the whole preview would be one long `unevaluated` list.
#[test]
fn secret_var_references_become_placeholder_literals() {
    let mut config = serde_json::json!({
        "client_id": "id-123",
        "client_secret_var": "TOAST_SECRET",
        "restaurant_guids": ["g-1"],
        "auth": { "token_var": "REST_TOKEN" },
        "endpoints": [{ "name": "charges", "key_var": "NESTED_KEY" }],
    });
    substitute_secret_vars(&mut config);

    assert!(
        config.get("client_secret_var").is_none(),
        "`_var` is stripped"
    );
    assert!(
        config["client_secret"].is_string(),
        "the literal field it names is filled in"
    );
    assert!(config["auth"]["token"].is_string(), "nested objects too");
    assert!(
        config["endpoints"][0]["key"].is_string(),
        "and objects inside arrays"
    );
    assert_eq!(
        config["client_id"], "id-123",
        "non-credential fields are untouched"
    );
}

/// A spec that already carries the literal keeps it — the placeholder fills a
/// gap, it does not overwrite an author's value.
#[test]
fn an_explicit_literal_survives_substitution() {
    let mut config = serde_json::json!({
        "client_secret": "literal-in-yaml",
        "client_secret_var": "TOAST_SECRET",
    });
    substitute_secret_vars(&mut config);
    assert_eq!(config["client_secret"], "literal-in-yaml");
    assert!(config.get("client_secret_var").is_none());
}
