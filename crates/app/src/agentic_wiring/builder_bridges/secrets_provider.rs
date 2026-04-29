use agentic_builder::BuilderSecretsProvider;
use oxy::adapters::secrets::SecretsManager;

pub struct OxyBuilderSecretsProvider {
    secrets_manager: SecretsManager,
}

impl OxyBuilderSecretsProvider {
    pub fn new(secrets_manager: SecretsManager) -> Self {
        Self { secrets_manager }
    }
}

impl BuilderSecretsProvider for OxyBuilderSecretsProvider {
    fn secrets_manager(&self) -> &SecretsManager {
        &self.secrets_manager
    }
}
