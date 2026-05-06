mod database;
mod environment;
mod manager;
mod storage;

pub use database::SecretsDatabaseStorage;
pub use manager::SecretsManager;
pub use storage::SecretsStorage;

// Re-export `OrgSecretsService` from `oxy-platform` under its legacy
// `oxy::adapters::secrets::OrgSecretsService` path. The implementation moved
// to platform so leaf integrations (airhouse, future ones) can use it without
// depending on the full `oxy` crate.
pub use oxy_platform::secrets::OrgSecretsService;
