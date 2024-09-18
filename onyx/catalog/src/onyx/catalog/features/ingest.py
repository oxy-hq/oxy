import uuid
from datetime import datetime

from onyx.catalog.adapters.connector.registry import ConnectorRegistry
from onyx.catalog.ingest.adapters.gmail import GmailOAuthConfig, GmailSource
from onyx.catalog.ingest.adapters.openai import OpenAIEncoder
from onyx.catalog.ingest.base.context import IngestRequest
from onyx.catalog.ingest.base.controller import IngestController
from onyx.catalog.ingest.base.types import Identity
from onyx.catalog.models.errors import (
    ConnectionAreBeingSynced,
    ConnectionNotFound,
    IntegrationNotFound,
    SourceNotSupported,
)
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.adapters.orm.errors import RowLockedError
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Message
from onyx.shared.models.constants import ConnectionSyncStatus, IntegrationSlugChoices
from onyx.shared.services.dispatcher import AbstractDispatcher

logger = get_logger(__name__)


class IngestIntegration(Message[bool]):
    integration_id: uuid.UUID


async def ingest_integration(
    request: IngestIntegration,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    config: OnyxConfig,
    dispatcher: AbstractDispatcher,
):
    integration = uow.integrations.get_by_id(request.integration_id)
    if not integration:
        raise IntegrationNotFound(f"Integration {request.integration_id} not found")
    identity = Identity(
        slug=integration.slug, namespace_id=str(integration.namespace_id), datasource_id=str(integration.id)
    )

    configuration = secrets_manager.decrypt_dict(integration.configuration)
    if integration.slug == IntegrationSlugChoices.gmail:
        source = GmailSource(
            GmailOAuthConfig.model_validate(
                {
                    "client_id": config.integration.gmail_client_id,
                    "client_secret": config.integration.gmail_client_secret,
                    "refresh_token": configuration.get("refresh_token"),
                }
            )
        )
    else:
        raise SourceNotSupported(f"Integration type: {integration.slug} not supported")

    ingest_controller = IngestController(
        config=config,
        dispatcher=dispatcher,
        encoder=OpenAIEncoder(config),
        uow=uow,
    )
    await ingest_controller.ingest(
        source=source,
        request=IngestRequest(
            identity=identity,
        ),
    )
    return True


class IngestConnection(Message[bool]):
    organization_id: uuid.UUID
    connection_id: uuid.UUID


def ingest_connection(
    request: IngestConnection,
    uow: AbstractUnitOfWork,
    secrets_manager: AbstractSecretsManager,
    connector_registry: ConnectorRegistry,
):
    __sync_connection(request.organization_id, request.connection_id, uow, secrets_manager, connector_registry)
    return True


def __sync_connection(
    organization_id: uuid.UUID,
    connection_id: uuid.UUID,
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


class UpdateConnectionSyncState(Message[bool]):
    connection_id: uuid.UUID
    error: str | None = None


def update_connection_sync_state(
    request: UpdateConnectionSyncState,
    uow: AbstractUnitOfWork,
):
    connection = uow.connections.get_for_update(request.connection_id)
    if not connection:
        raise ConnectionNotFound(f"Connection {request.connection_id} not found")
    connection.sync_error = request.error
    connection.sync_status = ConnectionSyncStatus.success if not request.error else ConnectionSyncStatus.error
    return True


class UpdateIntegrationSyncState(Message[bool]):
    integration_id: uuid.UUID
    error: str | None = None


def update_integration_sync_state(
    request: UpdateIntegrationSyncState,
    uow: AbstractUnitOfWork,
):
    ingest_state = uow.integrations.get_ingest_state_for_update(request.integration_id)
    if not ingest_state:
        raise IntegrationNotFound(f"Integration {request.integration_id} not found")

    ingest_state.sync_error = request.error
    ingest_state.sync_status = ConnectionSyncStatus.success if not request.error else ConnectionSyncStatus.error
    ingest_state.last_synced_at = datetime.now()
    return True
