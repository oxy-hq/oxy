from abc import ABC, abstractmethod

from onyx.catalog.adapters.pipeline._strategies import (
    FilePipelineStrategy,
    GmailPipelineStrategy,
    NotionPipelineStrategy,
    PipelineStrategy,
    SalesforcePipelineStrategy,
    SlackPipelineStrategy,
)
from onyx.catalog.adapters.pipeline.warehouse_config import AbstractWarehouseConfig
from onyx.catalog.models.integration import Integration
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager
from onyx.shared.config import OnyxConfig
from onyx.shared.models.constants import IntegrationSlugChoices


class AbstractPipelineEnvManager(ABC):
    @abstractmethod
    def get_ingest_env(self, integration: Integration) -> dict[str, str]:
        pass

    @abstractmethod
    def get_embed_env(self, integration: Integration) -> dict[str, str]:
        pass


class MeltanoEnvManager(AbstractPipelineEnvManager):
    def __init__(
        self,
        secrets_manager: AbstractSecretsManager,
        warehouse_config: AbstractWarehouseConfig,
        config: OnyxConfig,
    ) -> None:
        self.__strategies: dict[IntegrationSlugChoices, PipelineStrategy] = {
            IntegrationSlugChoices.salesforce: SalesforcePipelineStrategy(
                secrets_manager=secrets_manager,
                warehouse_config=warehouse_config,
                openai_config=config.openai,
                client_id=config.integration.salesforce_client_id,
                client_secret=config.integration.salesforce_client_secret,
            ),
            IntegrationSlugChoices.gmail: GmailPipelineStrategy(
                secrets_manager=secrets_manager,
                warehouse_config=warehouse_config,
                openai_config=config.openai,
                client_id=config.integration.gmail_client_id,
                client_secret=config.integration.gmail_client_secret,
            ),
            IntegrationSlugChoices.slack: SlackPipelineStrategy(
                secrets_manager=secrets_manager,
                warehouse_config=warehouse_config,
                openai_config=config.openai,
            ),
            IntegrationSlugChoices.notion: NotionPipelineStrategy(
                secrets_manager=secrets_manager,
                warehouse_config=warehouse_config,
                openai_config=config.openai,
            ),
            IntegrationSlugChoices.file: FilePipelineStrategy(
                secrets_manager=secrets_manager,
                warehouse_config=warehouse_config,
                s3_config=config.s3,
                openai_config=config.openai,
            ),
        }

    def __get_strategy(self, slug: IntegrationSlugChoices) -> PipelineStrategy:
        strategy = self.__strategies.get(slug)
        if not strategy:
            raise NotImplementedError(f"Integration {slug} not supported")
        return strategy

    def get_ingest_env(self, integration: Integration) -> dict[str, str]:
        strategy = self.__get_strategy(integration.slug)
        env = strategy.get_ingest_env(integration=integration)
        return env

    def get_embed_env(self, integration: Integration) -> dict[str, str]:
        strategy = self.__get_strategy(integration.slug)
        env = strategy.get_embed_env(integration=integration)
        return env
