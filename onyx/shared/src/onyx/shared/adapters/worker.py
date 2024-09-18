import uuid
from abc import ABC, abstractmethod

from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from onyx.shared.models.types import IngestActivityInput, IngestConnectionActivityInput, SupportedWorkflows
from temporalio.client import Client


class AbstractWorker(ABC):
    @abstractmethod
    async def sync(self, connection_id: uuid.UUID, organization_id: uuid.UUID):
        pass

    @abstractmethod
    async def ingest(self, integration_id: uuid.UUID):
        pass


class TemporalWorker(Logged, AbstractWorker):
    def __init__(self, config: OnyxConfig) -> None:
        self.url = config.temporal.url
        self.namespace = config.temporal.namespace
        self.catalog_queue = config.temporal.catalog_queue
        self.api_key = config.temporal.api_key or None
        self.tls = config.temporal.tls

    async def _request(self, queue: str, workflow: str, id: str, data):
        client = await Client.connect(
            self.url,
            api_key=self.api_key,
            namespace=self.namespace,
            tls=self.tls,
        )
        result = await client.start_workflow(
            workflow,
            data,
            id=id,
            task_queue=queue,
        )
        return result

    async def sync(self, connection_id: uuid.UUID, organization_id: uuid.UUID):
        self.log.info(f"Syncing connection {connection_id} for organization {organization_id}")
        data = IngestConnectionActivityInput(
            connection_id=str(connection_id),
            organization_id=str(organization_id),
        )
        return await self._request(
            self.catalog_queue, SupportedWorkflows.INGEST_CONNECTION, f"ingest-connection-{connection_id}", data
        )

    async def ingest(self, integration_id: uuid.UUID):
        self.log.info(f"Syncing integration {integration_id}")
        data = IngestActivityInput(
            integration_id=str(integration_id),
        )
        return await self._request(
            self.catalog_queue, SupportedWorkflows.INGEST, f"ingest-integration-{integration_id}", data
        )
