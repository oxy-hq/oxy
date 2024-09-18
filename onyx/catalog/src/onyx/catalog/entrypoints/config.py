from typing import Iterable

from onyx.app import AbstractOnyxApp
from onyx.catalog.adapters.chat_client import AbstractChatClient, ChatClient
from onyx.catalog.adapters.connector.bigquery import BigQueryConnector
from onyx.catalog.adapters.connector.clickhouse import ClickhouseConnector
from onyx.catalog.adapters.connector.registry import ConnectorRegistry
from onyx.catalog.adapters.connector.snowflake import SnowflakeConnector
from onyx.catalog.adapters.gmail import AbstractGmail, Gmail
from onyx.catalog.adapters.notion import AbstractNotion, Notion
from onyx.catalog.adapters.pipeline.env_manager import AbstractPipelineEnvManager, MeltanoEnvManager
from onyx.catalog.adapters.pipeline.warehouse_config import AbstractWarehouseConfig, WarehouseConfig
from onyx.catalog.adapters.ranking.ranking import AbstractRankingAdapter, GPTRankingAdapter
from onyx.catalog.adapters.salesforce import AbstractSalesforce, Salesforce
from onyx.catalog.adapters.search import AbstractSearchClient, VespaSearchClient
from onyx.catalog.adapters.slack import AbstractSlack, Slack
from onyx.catalog.adapters.task_queues import (
    AbstractTaskQueuePublisher,
    AirflowPublisher,
)
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork, UnitOfWork
from onyx.shared.adapters.notify import AbstractNotification, ConsoleNotification, SlackNotification
from onyx.shared.adapters.orm.database import create_engine, read_session_factory, sqlalchemy_session_maker
from onyx.shared.adapters.orm.mixins import sql_uow_factory
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager, SOPSSecretsManager
from onyx.shared.adapters.worker import AbstractWorker, TemporalWorker
from onyx.shared.config import OnyxConfig
from onyx.shared.models.constants import ConnectionSlugChoices
from onyx.shared.services.base import DependencyRegistration
from sqlalchemy.orm import Session


def config_mapper(config: OnyxConfig, app: AbstractOnyxApp) -> Iterable[DependencyRegistration]:
    engine = create_engine(config.database)
    write_session_factory = sqlalchemy_session_maker(engine=engine)
    chat_client = ChatClient(app.chat)
    notification_cls = SlackNotification

    if config.stage.is_local():
        notification_cls = ConsoleNotification
    connector_registry = ConnectorRegistry(
        (ConnectionSlugChoices.snowflake, SnowflakeConnector),
        (ConnectionSlugChoices.bigquery, BigQueryConnector),
        (ConnectionSlugChoices.clickhouse, ClickhouseConnector),
    )

    return (
        DependencyRegistration(OnyxConfig, config, True),
        DependencyRegistration(AbstractChatClient, chat_client, True),
        DependencyRegistration(Session, read_session_factory(engine)),
        DependencyRegistration(
            AbstractUnitOfWork, sql_uow_factory(session_factory=write_session_factory, cls=UnitOfWork)
        ),
        DependencyRegistration(AbstractWarehouseConfig, WarehouseConfig),
        DependencyRegistration(AbstractSecretsManager, SOPSSecretsManager),
        DependencyRegistration(AbstractTaskQueuePublisher, AirflowPublisher),
        DependencyRegistration(AbstractPipelineEnvManager, MeltanoEnvManager),
        DependencyRegistration(AbstractSalesforce, Salesforce),
        DependencyRegistration(AbstractGmail, Gmail),
        DependencyRegistration(AbstractSlack, Slack),
        DependencyRegistration(AbstractNotion, Notion),
        DependencyRegistration(AbstractRankingAdapter, GPTRankingAdapter),
        DependencyRegistration(AbstractSearchClient, VespaSearchClient),
        DependencyRegistration(AbstractNotification, notification_cls),
        DependencyRegistration(ConnectorRegistry, connector_registry, True),
        DependencyRegistration(AbstractWorker, TemporalWorker),
    )
