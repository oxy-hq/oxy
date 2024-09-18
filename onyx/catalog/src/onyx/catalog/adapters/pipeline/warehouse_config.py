import abc

from onyx.shared.config import OnyxConfig
from onyx.shared.models.constants import EnvConfigType


class AbstractWarehouseConfig(abc.ABC):
    @abc.abstractmethod
    def generate_tap_env(self, db_name: str) -> EnvConfigType:
        ...

    @abc.abstractmethod
    def generate_target_env(self, db_name: str) -> EnvConfigType:
        ...

    @abc.abstractmethod
    def generate_dbt_env(self, db_name: str) -> EnvConfigType:
        ...

    @abc.abstractmethod
    def generate_embed_target_env(self, target_schema: str, target_table: str) -> EnvConfigType:
        ...


class WarehouseConfig(AbstractWarehouseConfig):
    def __init__(
        self,
        config: OnyxConfig,
    ) -> None:
        clickhouse_config = config.clickhouse
        self.__clickhouse_host = clickhouse_config.host
        self.__clickhouse_port = clickhouse_config.port
        self.__clickhouse_username = clickhouse_config.username
        self.__clickhouse_password = clickhouse_config.password
        self.__clickhouse_protocol = clickhouse_config.protocol
        self.__clickhouse_database = clickhouse_config.database
        self.__clickhouse_secure = clickhouse_config.secure
        vespa_config = config.vespa
        self.__vespa_url = vespa_config.url
        self.__vespa_cloud_secret_token = vespa_config.cloud_secret_token

    def __sqlalchemy_uri(self, db_name: str) -> str:
        uri = (
            f"{self.__clickhouse_protocol}://{self.__clickhouse_username}:{self.__clickhouse_password}"
            f"@{self.__clickhouse_host}:{self.__clickhouse_port}/{db_name}"
        )

        if self.__clickhouse_secure:
            uri += "?protocol=https"

        return uri

    def generate_tap_env(self, db_name: str):
        return {
            "TAP_CLICKHOUSE_HOST": self.__clickhouse_host,
            "TAP_CLICKHOUSE_PORT": str(self.__clickhouse_port),
            "TAP_CLICKHOUSE_USER": self.__clickhouse_username,
            "TAP_CLICKHOUSE_PASSWORD": self.__clickhouse_password,
            "TAP_CLICKHOUSE_DATABASE": db_name,
            "TAP_CLICKHOUSE_SECURE": str(self.__clickhouse_secure),
            # TODO: Temporary disable until we install ssl certificates
            "TAP_CLICKHOUSE_VERIFY": str(False),
        }

    def generate_target_env(self, db_name: str):
        return {
            "TARGET_CLICKHOUSE_SQLALCHEMY_URL": self.__sqlalchemy_uri(self.__clickhouse_database),
            "TARGET_CLICKHOUSE_DEFAULT_TARGET_SCHEMA": db_name,
        }

    def generate_embed_target_env(self, target_schema: str, target_table: str):
        secret_kwargs = {}
        if self.__vespa_cloud_secret_token:
            secret_kwargs["TARGET_VESPA_CLOUD_SECRET_TOKEN"] = self.__vespa_cloud_secret_token

        return {
            "TARGET_VESPA_NAMESPACE": target_schema,
            "TARGET_VESPA_GROUP_NAME": target_table,
            "TARGET_VESPA_VESPA_URL": self.__vespa_url,
            **secret_kwargs,
        }

    def generate_dbt_env(self, db_name: str):
        return {
            "DBT_CLICKHOUSE_DATABASE": db_name,
            "DBT_CLICKHOUSE_HOST": self.__clickhouse_host,
            "DBT_CLICKHOUSE_PORT": str(self.__clickhouse_port),
            "DBT_CLICKHOUSE_SECURE": str(self.__clickhouse_secure),
            "DBT_CLICKHOUSE_USERNAME": self.__clickhouse_username,
            "DBT_CLICKHOUSE_PASSWORD": self.__clickhouse_password,
        }
