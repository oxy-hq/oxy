use oxy::adapters::secrets::SecretsManager;

/// Provides a [`SecretsManager`] to builder tools that need to resolve
/// credentials (e.g. `run_dbt_models` loading an Oxy warehouse adapter).
///
/// Implemented by the host adapter in `app::agentic_wiring` and injected via
/// [`BuilderPipelineParams`](crate::pipeline::BuilderPipelineParams).
pub trait BuilderSecretsProvider: Send + Sync {
    fn secrets_manager(&self) -> &SecretsManager;
}
