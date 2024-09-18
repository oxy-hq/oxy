from abc import ABC, abstractmethod
from uuid import UUID

from onyx.shared.services.base import Service


class AbstractWarehouseClient(ABC):
    @abstractmethod
    async def run_query(self, organization_id: UUID, connection_id: UUID, query: str) -> list:
        pass


class WarehouseClient(AbstractWarehouseClient):
    def __init__(self, catalog_service: Service):
        self.catalog_service = catalog_service

    async def run_query(self, organization_id: UUID, connection_id: UUID, query: str) -> list:
        from onyx.catalog.features.data_sources import RunQuery

        return await self.catalog_service.handle(
            RunQuery(organization_id=organization_id, connection_id=connection_id, query=query)
        )


class FakeWarehouseClient(AbstractWarehouseClient):
    async def run_query(self, organization_id: UUID, connection_id: UUID, query: str) -> list:
        return []
