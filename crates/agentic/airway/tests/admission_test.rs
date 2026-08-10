//! A tightened `contract_policy` must refuse a connector that declares no
//! contract, and the refusal must name the resource.
//!
//! The unit tests in `boxed.rs` pin the forwarding; this pins that the
//! forwarding is actually *reached* from the constructor the worker calls.

use airway::connector::{ContractPolicy, Environment, ResourceInfo, SourceConnector};
use airway::types::WriteDisposition;
use async_trait::async_trait;

struct SilentConnector;

#[async_trait]
impl SourceConnector for SilentConnector {
    fn name(&self) -> &str {
        "silent"
    }

    fn resources(&self) -> Vec<ResourceInfo> {
        vec![ResourceInfo {
            name: "charges".to_string(),
            description: None,
            write_disposition: WriteDisposition::Merge,
            primary_key: Some(vec!["id".to_string()]),
            cursor_field: Some("created".to_string()),
        }]
    }

    async fn extract(
        &self,
        _resource: &str,
        _state: Option<&serde_json::Value>,
    ) -> Result<airway::connector::ExtractionResult, airway::AirwayError> {
        unimplemented!("not exercised")
    }
}

#[test]
fn require_declared_refuses_an_undeclared_resource_and_names_it() {
    let result = airway::Source::try_from_connector_with(
        agentic_airway::boxed::BoxedSourceConnector(Box::new(SilentConnector)),
        ContractPolicy::RequireDeclared,
        Environment::Production,
    );

    let err = match result {
        Ok(_) => panic!("an undeclared cursored resource must be refused"),
        Err(e) => e,
    };

    let msg = err.to_string();
    assert!(msg.contains("charges"), "must name the resource: {msg}");
    assert!(msg.contains("silent"), "must name the connector: {msg}");
}

#[test]
fn permissive_admits_the_same_connector() {
    airway::Source::try_from_connector_with(
        agentic_airway::boxed::BoxedSourceConnector(Box::new(SilentConnector)),
        ContractPolicy::Permissive,
        Environment::Production,
    )
    .expect("permissive is today's behaviour and must admit everything");
}
