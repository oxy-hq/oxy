from uuid import UUID

from onyx.catalog.adapters.connector.registry import ConnectorRegistry
from onyx.catalog.adapters.task_queues import AbstractTaskQueuePublisher
from onyx.catalog.models.commands import ConnectionConfiguration, IntegrationConfiguration
from onyx.catalog.models.connection import Connection
from onyx.catalog.models.errors import (
    ConnectionAreBeingSynced,
    ConnectionNotFound,
    IntegrationAreBeingSynced,
    IntegrationNotFound,
)
from onyx.catalog.models.integration import Integration
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.adapters.orm.errors import RowLockedError
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager
from onyx.shared.adapters.worker import AbstractWorker
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Command, Event
from onyx.shared.models.constants import ConnectionSyncStatus, IntegrationSlugChoices
from onyx.shared.services.dispatcher import AbstractDispatcher
from onyx.shared.services.message_bus import EventCollector

logger = get_logger(__name__)


class DeleteIntegration(Command[None]):
    id: str


def delete_integration(request: DeleteIntegration, uow: AbstractUnitOfWork):
    uow.namespaces.delete_integration(request.id)


class CreateIntegration(Command[Integration]):
    organization_id: UUID
    created_by: UUID
    name: str
    configuration: IntegrationConfiguration
    is_private: bool
    integration_metadata: dict


def create_integration(
    request: CreateIntegration,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    events_collector: EventCollector,
):
    namespace = __get_namespace(
        request.organization_id,
        request.created_by,
        request.is_private,
        uow,
    )

    integration = Integration(
        slug=request.configuration.root.slug,
        organization_id=request.organization_id,
        name=request.name,
        configuration=secrets_manager.encrypt_dict(request.configuration.model_dump_json()),
        namespace_id=namespace.id,
        namespace=namespace,
        integration_metadata=request.integration_metadata,
    )
    uow.integrations.add(integration)
    uow.commit()

    if integration.id:
        events_collector.publish(
            IntegrationCreated(
                integration_id=integration.id,
                organization_id=integration.organization_id,
            )
        )

    return integration


class SyncIntegration(Command[None]):
    organization_id: UUID
    integration_id: UUID


async def sync_integration(
    request: SyncIntegration,
    worker: AbstractWorker,
    config: OnyxConfig,
    uow: AbstractUnitOfWork,
    task_queue: AbstractTaskQueuePublisher,
    dispatcher: AbstractDispatcher,
):
    await __sync_integration_or_dispatch(
        request.organization_id, request.integration_id, uow, task_queue, config, worker, dispatcher
    )


class IntegrationCreated(Event):
    organization_id: UUID
    integration_id: UUID


async def integration_created(
    event: IntegrationCreated,
    worker: AbstractWorker,
    config: OnyxConfig,
    uow: AbstractUnitOfWork,
    task_queue: AbstractTaskQueuePublisher,
    dispatcher: AbstractDispatcher,
):
    await __sync_integration_or_dispatch(
        event.organization_id, event.integration_id, uow, task_queue, config, worker, dispatcher
    )


def __get_namespace(organization_id: UUID, created_by: UUID, is_private: bool, uow: AbstractUnitOfWork):
    if not is_private:
        return uow.namespaces.get_default_namespace(organization_id)

    return uow.namespaces.get_private_namespace(organization_id, created_by)


async def __sync_integration_or_dispatch(
    organization_id: UUID,
    integration_id: UUID,
    uow: AbstractUnitOfWork,
    task_queue: AbstractTaskQueuePublisher,
    config: OnyxConfig,
    worker: AbstractWorker,
    dispatcher: AbstractDispatcher,
):
    integration = uow.integrations.get_by_id(integration_id)
    if not integration:
        raise IntegrationNotFound(f"Integration {integration_id} not found")
    if config.temporal.enabled and integration.slug in [IntegrationSlugChoices.gmail]:
        try:
            await worker.ingest(integration_id)
        except Exception as exc:
            ingest_state = uow.integrations.get_or_create_ingest_state(integration_id)
            ingest_state.sync_status = ConnectionSyncStatus.error
            ingest_state.sync_error = f"Failed to sync integration: {exc}"
            uow.commit()
    else:
        await dispatcher.dispatch(__sync_integration, organization_id, integration_id, uow, task_queue)


def __sync_integration(
    organization_id: UUID,
    integration_id: UUID,
    uow: AbstractUnitOfWork,
    task_queue: AbstractTaskQueuePublisher,
):
    try:
        integration = uow.integrations.get_for_update(integration_id)
        if not integration:
            raise IntegrationNotFound(f"Integration {integration_id} not found")

        task = uow.integrations.get_latest_task(integration_id)
        if task:
            logger.info(f"Checking if integration {integration.slug} {task.external_id} is being synced")
            is_syncing = task_queue.is_task_running(task.external_id, str(integration.slug))
            if is_syncing:
                raise IntegrationAreBeingSynced(f"Integration {integration_id} is being synced")

        new_task = task_queue.publish_integration_created(integration)
        logger.info(f"Published task {new_task.external_id} to {new_task.queue_system}")
        uow.tasks.add(new_task)
        uow.commit()
    except RowLockedError as e:
        raise IntegrationAreBeingSynced(f"Integration {integration_id} is being synced") from e


class RunQuery(Command[list]):
    organization_id: UUID
    connection_id: UUID
    query: str


def run_query(
    command: RunQuery,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    connector_registry: ConnectorRegistry,
):
    connection = uow.connections.get_by_id(command.connection_id)
    if not connection or not connection.id:
        raise ConnectionNotFound(f"Connection {command.connection_id} not found")

    configuration = secrets_manager.decrypt_dict(connection.configuration)
    logger.info(f"Running query {command.query} on connection {configuration}")
    connector = connector_registry.get_connector_cls(connection.slug)(
        str(connection.organization_id), str(connection.id), configuration
    )
    with connector:
        return connector.query(command.query)


class TestConnection(Command[bool]):
    configuration: ConnectionConfiguration


def test_connection(
    request: TestConnection,
    connector_registry: ConnectorRegistry,
):
    configuration = request.configuration.model_dump()
    connector = connector_registry.get_connector_cls(request.configuration.root.slug)("", "", configuration)
    return connector.test_connection()


class CreateConnection(Command[Connection]):
    organization_id: UUID
    created_by: UUID
    name: str
    configuration: ConnectionConfiguration
    is_private: bool
    connection_metadata: dict


def create_connection(
    create_connection_command: CreateConnection,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    event_collector: EventCollector,
):
    namespace = __get_namespace(
        create_connection_command.organization_id,
        create_connection_command.created_by,
        create_connection_command.is_private,
        uow,
    )
    logger.debug(f"Creating connection {create_connection_command.configuration.model_dump_json()}")
    connection = Connection(
        slug=create_connection_command.configuration.root.slug,
        organization_id=create_connection_command.organization_id,
        name=create_connection_command.name,
        configuration=secrets_manager.encrypt_dict(create_connection_command.configuration.model_dump_json()),
        namespace_id=namespace.id,
        namespace=namespace,
        connection_metadata=create_connection_command.connection_metadata,
        sync_status=ConnectionSyncStatus.syncing,
    )
    uow.connections.add(connection)
    uow.commit()

    if connection.id:
        event_collector.publish(
            ConnectionCreated(
                organization_id=create_connection_command.organization_id,
                connection_id=connection.id,
            )
        )

    return connection


class UpdateConnection(Command[Connection]):
    id: UUID
    name: str
    configuration: ConnectionConfiguration
    connection_metadata: dict


def update_connection(
    update_connection_command: UpdateConnection,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    event_collector: EventCollector,
):
    connection = uow.connections.get_for_update(update_connection_command.id)
    if not connection:
        raise ConnectionNotFound(f"Connection {update_connection_command.id} not found")

    configuration = secrets_manager.decrypt_dict(connection.configuration)

    credentials_key = update_connection_command.configuration.root.credentials_key
    if not credentials_key:
        credentials_key = configuration["credentials_key"]

    update_configuration = ConnectionConfiguration(
        {
            "slug": "bigquery",
            "database": update_connection_command.configuration.root.database,
            "dataset": update_connection_command.configuration.root.dataset,
            "credentials_key": credentials_key,
        }
    )

    connection.configuration = secrets_manager.encrypt_dict(update_configuration.model_dump_json())
    connection.name = update_connection_command.name
    connection.connection_metadata = update_connection_command.connection_metadata
    connection.sync_status = ConnectionSyncStatus.syncing
    connection.sync_error = None
    uow.commit()

    event_collector.publish(
        ConnectionCreated(
            organization_id=connection.organization_id,
            connection_id=connection.id,
        )
    )

    return connection


class ConnectionCreated(Event):
    organization_id: UUID
    connection_id: UUID


async def connection_created(
    event: ConnectionCreated,
    worker: AbstractWorker,
    config: OnyxConfig,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    connector_registry: ConnectorRegistry,
    dispatcher: AbstractDispatcher,
):
    await __sync_connection_or_dispatch(
        event.connection_id,
        event.organization_id,
        uow,
        secrets_manager,
        connector_registry,
        dispatcher,
        worker,
        config,
    )


class SyncConnection(Command[bool]):
    organization_id: UUID
    connection_id: UUID


async def sync_connection(
    request: SyncConnection,
    uow: AbstractUnitOfWork,
    worker: AbstractWorker,
    config: OnyxConfig,
    secrets_manager: AbstractSecretsManager,
    connector_registry: ConnectorRegistry,
    dispatcher: AbstractDispatcher,
):
    await __sync_connection_or_dispatch(
        request.connection_id,
        request.organization_id,
        uow,
        secrets_manager,
        connector_registry,
        dispatcher,
        worker,
        config,
    )


async def __sync_connection_or_dispatch(
    connection_id: UUID,
    organization_id: UUID,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    connector_registry: ConnectorRegistry,
    dispatcher: AbstractDispatcher,
    worker: AbstractWorker,
    config: OnyxConfig,
):
    if config.temporal.enabled:
        connection = uow.connections.get_by_id(connection_id)
        if not connection:
            raise ConnectionNotFound(f"Connection {connection_id} not found")
        try:
            await worker.sync(connection_id, organization_id)
        except Exception as exc:
            connection.sync_status = ConnectionSyncStatus.error
            connection.sync_error = f"Failed to sync connection: {exc}"
            uow.commit()

    else:
        await dispatcher.dispatch(
            __sync_connection, organization_id, connection_id, uow, secrets_manager, connector_registry
        )


def __sync_connection(
    organization_id: UUID,
    connection_id: UUID,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    connector_registry: ConnectorRegistry,
):
    try:
        logger.info(f"Syncing connection {connection_id}")
        connection = uow.connections.get_for_update(connection_id)
        if not connection:
            raise ConnectionNotFound(f"Connection {connection_id} not found")
        connection.sync_status = ConnectionSyncStatus.syncing
        connection.sync_error = None

        uow.commit()
        configuration = secrets_manager.decrypt_dict(connection.configuration)
        logger.debug(f"Configuration {configuration}")
        connector = connector_registry.get_connector_cls(connection.slug)(
            str(connection.organization_id), str(connection_id), configuration
        )
        with connector:
            try:
                tables = connector.get_tables()
                response = connection.merge_children(tables, ("tables", "identity"))
                logger.info(f"Ingesting summary {response}")
                connection.sync_status = ConnectionSyncStatus.success

                uow.commit()
                return response
            except Exception as e:
                logger.error(f"Error syncing connection {connection_id}: {e}", exc_info=True)
                connection.sync_status = ConnectionSyncStatus.error
                connection.sync_error = str(e)
                uow.commit()

    except RowLockedError as e:
        raise ConnectionAreBeingSynced(f"Connection {connection_id} is being synced") from e
    except Exception as e:
        if connection:
            logger.error(f"Error syncing connection {connection_id}: {e}", exc_info=True)
            connection.sync_status = ConnectionSyncStatus.error
            connection.sync_error = str(e)
            uow.commit()
        raise e
