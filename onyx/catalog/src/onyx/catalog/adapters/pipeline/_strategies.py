import abc
import json
from typing import TypedDict, Unpack

from onyx.catalog.adapters.pipeline.warehouse_config import AbstractWarehouseConfig
from onyx.catalog.models.integration import Integration
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager
from onyx.shared.config import OpenAISettings, S3Settings
from onyx.shared.models.constants import EnvConfigType


class PipelineStrategyKwargs(TypedDict):
    secrets_manager: AbstractSecretsManager
    warehouse_config: AbstractWarehouseConfig
    openai_config: OpenAISettings


class PipelineStrategy(abc.ABC):
    def __init__(
        self,
        **kwargs: Unpack[PipelineStrategyKwargs],
    ) -> None:
        self.__secrets_manager = kwargs["secrets_manager"]
        self.__warehouse_config = kwargs["warehouse_config"]
        self.__openai_config = kwargs["openai_config"]

    @property
    def secrets_manager(self) -> AbstractSecretsManager:
        return self.__secrets_manager

    @property
    def warehouse_config(self) -> AbstractWarehouseConfig:
        return self.__warehouse_config

    @property
    def openai_config(self) -> OpenAISettings:
        return self.__openai_config

    @abc.abstractmethod
    def get_ingest_env(self, integration: Integration) -> EnvConfigType:
        ...

    @abc.abstractmethod
    def get_embed_env(self, integration: Integration) -> EnvConfigType:
        ...

    def get_embedding_env(self):
        return {
            "TARGET_VESPA_OPENAI_API_KEY": self.openai_config.api_key,
            "TARGET_VESPA_OPENAI_MODEL": self.openai_config.embeddings_model,
        }


class SalesforcePipelineStrategy(PipelineStrategy):
    def __init__(
        self,
        client_id: str,
        client_secret: str,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.__client_id = client_id
        self.__client_secret = client_secret

    def get_ingest_env(self, integration: Integration):
        configuration = self.secrets_manager.decrypt_dict(integration.configuration)
        target_env = self.warehouse_config.generate_target_env(integration.target_stg_schema)
        dbt_env = self.warehouse_config.generate_dbt_env(integration.target_prod_schema)
        return {
            "TAP_SALESFORCE_CLIENT_ID": self.__client_id,
            "TAP_SALESFORCE_CLIENT_SECRET": self.__client_secret,
            "TAP_SALESFORCE_REFRESH_TOKEN": configuration["refresh_token"],
            "DBT_SALESFORCE_SOURCE_SCHEMA": integration.target_stg_schema,
            "DBT_SLACK_SOURCE_SCHEMA": "",
            "DBT_NOTION_SOURCE_SCHEMA": "",
            **target_env,
            **dbt_env,
        }

    def get_embed_env(self, integration: Integration):
        tap_env = self.warehouse_config.generate_tap_env(integration.target_prod_schema)
        target_env = self.warehouse_config.generate_embed_target_env(
            integration.target_embedding_schema,
            integration.target_embedding_table,
        )
        embedding_env = self.get_embedding_env()
        opportunity_stream = f"{integration.target_prod_schema}-opportunity"
        activity_stream = f"{integration.target_prod_schema}-activity"

        return {
            **tap_env,
            **embedding_env,
            "TAP_CLICKHOUSE_STREAM_MAPS": json.dumps(
                {
                    opportunity_stream: {
                        "id": "opportunity_id",
                        "metadata": 'str({"source": opportunity_id, "source_type": "salesforce"})',
                    },
                    activity_stream: {
                        "id": "activity_id",
                        "metadata": 'str({"source": activity_id, "source_type": "salesforce"})',
                    },
                    "__else__": "__NULL__",
                }
            ),
            "TAP_CLICKHOUSE_SELECTED_STREAMS": json.dumps([opportunity_stream, activity_stream]),
            "TAP_CLICKHOUSE_STREAM_REPLICATION_KEYS": json.dumps(
                {
                    opportunity_stream: "updated_at",
                    activity_stream: "updated_at",
                }
            ),
            **target_env,
        }


class GmailPipelineStrategy(PipelineStrategy):
    def __init__(
        self,
        client_id: str,
        client_secret: str,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.__client_id = client_id
        self.__client_secret = client_secret

    def get_ingest_env(self, integration: Integration):
        configuration = self.secrets_manager.decrypt_dict(integration.configuration)
        target_env = self.warehouse_config.generate_target_env(integration.target_stg_schema)
        query = configuration.get("query")
        return {
            "TAP_GMAIL_OAUTH_CREDENTIALS_CLIENT_ID": self.__client_id,
            "TAP_GMAIL_OAUTH_CREDENTIALS_CLIENT_SECRET": self.__client_secret,
            "TAP_GMAIL_OAUTH_CREDENTIALS_REFRESH_TOKEN": configuration["refresh_token"],
            "TAP_GMAIL_MESSAGES_Q": query,
            "TAP_GMAIL_USER_ID": "me",
            **target_env,
        }

    def get_embed_env(self, integration: Integration):
        tap_env = self.warehouse_config.generate_tap_env(integration.target_stg_schema)
        target_env = self.warehouse_config.generate_embed_target_env(
            integration.target_embedding_schema,
            integration.target_embedding_table,
        )
        embedding_env = self.get_embedding_env()
        message_stream = f"{integration.target_stg_schema}-messages"

        return {
            **tap_env,
            **embedding_env,
            "TAP_CLICKHOUSE_SELECTED_STREAMS": json.dumps([message_stream]),
            "TAP_CLICKHOUSE_STREAM_REPLICATION_KEYS": json.dumps(
                {
                    message_stream: "internal_date",
                }
            ),
            **target_env,
        }


class SlackPipelineStrategy(PipelineStrategy):
    def __init__(
        self,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)

    def get_ingest_env(self, integration: Integration):
        configuration = self.secrets_manager.decrypt_dict(integration.configuration)
        target_env = self.warehouse_config.generate_target_env(integration.target_stg_schema)
        dbt_env = self.warehouse_config.generate_dbt_env(integration.target_prod_schema)

        return {
            "TAP_SLACK_API_KEY": configuration["token"],
            "DBT_SLACK_SOURCE_SCHEMA": integration.target_stg_schema,
            "DBT_SALESFORCE_SOURCE_SCHEMA": "",
            "DBT_NOTION_SOURCE_SCHEMA": "",
            **target_env,
            **dbt_env,
        }

    def get_embed_env(self, integration: Integration):
        tap_env = self.warehouse_config.generate_tap_env(integration.target_prod_schema)
        target_env = self.warehouse_config.generate_embed_target_env(
            integration.target_embedding_schema,
            integration.target_embedding_table,
        )
        embedding_env = self.get_embedding_env()
        channel_joined_stream = f"{integration.target_prod_schema}-sl_channel_joined"

        return {
            **tap_env,
            **embedding_env,
            "TAP_CLICKHOUSE_SELECTED_STREAMS": json.dumps([channel_joined_stream]),
            "TAP_CLICKHOUSE_STREAM_REPLICATION_KEYS": json.dumps({channel_joined_stream: "message_created_at"}),
            **target_env,
        }


class NotionPipelineStrategy(PipelineStrategy):
    def get_ingest_env(self, integration: Integration):
        configuration = self.secrets_manager.decrypt_dict(integration.configuration)
        target_env = self.warehouse_config.generate_target_env(integration.target_stg_schema)
        dbt_env = self.warehouse_config.generate_dbt_env(integration.target_prod_schema)

        return {
            "TAP_NOTION_TOKEN": configuration["token"],
            "DBT_NOTION_SOURCE_SCHEMA": integration.target_stg_schema,
            "DBT_SALESFORCE_SOURCE_SCHEMA": "",
            "DBT_SLACK_SOURCE_SCHEMA": "",
            **target_env,
            **dbt_env,
        }

    def get_embed_env(self, integration: Integration):
        tap_env = self.warehouse_config.generate_tap_env(integration.target_stg_schema)
        target_env = self.warehouse_config.generate_embed_target_env(
            integration.target_embedding_schema,
            integration.target_embedding_table,
        )
        embedding_env = self.get_embedding_env()
        page_stream = f"{integration.target_prod_schema}-notion_page_aggregation"

        return {
            **tap_env,
            **embedding_env,
            "TAP_CLICKHOUSE_SELECTED_STREAMS": json.dumps([page_stream]),
            "TAP_CLICKHOUSE_STREAM_REPLICATION_KEYS": json.dumps({page_stream: "last_edited_time"}),
            **target_env,
        }


class FilePipelineStrategy(PipelineStrategy):
    def __init__(
        self,
        s3_config: S3Settings,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.__s3_config = s3_config

    def get_ingest_env(self, integration: Integration):
        configuration = self.secrets_manager.decrypt_dict(integration.configuration)
        target_env = self.warehouse_config.generate_target_env(integration.target_stg_schema)
        dbt_env = self.warehouse_config.generate_dbt_env(integration.target_prod_schema)
        return {
            **target_env,
            **dbt_env,
            "TAP_FILE_KEY": configuration["path"],
            "TAP_FILE_NAME": integration.name,
            "TAP_FILE_ENDPOINT": self.__s3_config.endpoint,
            "TAP_FILE_REGION": self.__s3_config.region,
            "TAP_FILE_USE_SSL": str(self.__s3_config.use_ssl),
            "TAP_FILE_BUCKET_NAME": self.__s3_config.bucket_name,
            "TAP_FILE_ACCESS_KEY_ID": self.__s3_config.access_key_id,
            "TAP_FILE_SECRET_ACCESS_KEY": self.__s3_config.secret_access_key,
        }

    def get_embed_env(self, integration: Integration):
        tap_env = self.warehouse_config.generate_tap_env(integration.target_stg_schema)
        target_env = self.warehouse_config.generate_embed_target_env(
            integration.target_embedding_schema,
            integration.target_embedding_table,
        )
        embedding_env = self.get_embedding_env()

        return {
            **tap_env,
            **embedding_env,
            **target_env,
        }
