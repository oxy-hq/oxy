from abc import ABC, abstractmethod
from uuid import UUID

from onyx.shared.models.common import AgentDisplayInfo, AgentInfo
from onyx.shared.services.base import Service


class AbstractCatalogClient(ABC):
    @abstractmethod
    async def get_agent_info(self, agent_id: UUID, published: bool) -> AgentInfo:
        ...

    @abstractmethod
    async def get_agent_id_by_subdomain(self, subdomain: str) -> UUID | None:
        ...

    @abstractmethod
    async def get_published_versions_by_agent_ids(self, agent_ids: list[UUID]) -> dict[UUID, AgentDisplayInfo]:
        ...

    @abstractmethod
    async def get_random_published_agents(self) -> list[AgentDisplayInfo]:
        ...


class CatalogClient(AbstractCatalogClient):
    def __init__(self, catalog_service: Service):
        self.catalog_service = catalog_service

    async def get_agent_info(self, agent_id, published):
        from onyx.catalog.features.agent.queries import GetAgentInfo

        return await self.catalog_service.handle(GetAgentInfo(agent_id=agent_id, published=published))

    async def get_agent_id_by_subdomain(self, subdomain):
        from onyx.catalog.features.agent.queries import GetAgentIdByPublishedSubdomain

        return await self.catalog_service.handle(GetAgentIdByPublishedSubdomain(subdomain=subdomain))

    async def get_published_versions_by_agent_ids(self, agent_ids):
        from onyx.catalog.features.agent.queries import GetPublishedAgentVersionByAgentIds

        agents = await self.catalog_service.handle(GetPublishedAgentVersionByAgentIds(ids=agent_ids))
        return {
            agent["published_version"].agent_id: {
                "name": agent["published_version"].name,
                "subdomain": agent["published_version"].subdomain,
                "avatar": agent["published_version"].avatar,
                "is_deleted": agent["is_deleted"],
            }
            for agent in agents
            if agent and agent["published_version"]
        }

    async def get_random_published_agents(self) -> list[AgentDisplayInfo]:
        from onyx.catalog.features.agent.queries import GetRandomPublishedAgents

        return await self.catalog_service.handle(GetRandomPublishedAgents())


class FakeCatalogClient(AbstractCatalogClient):
    def __init__(self, agent_info: AgentInfo, agent_id: UUID):
        self.agent_id = agent_id
        self.agent_info = agent_info

    async def get_agent_info(self, agent_id, published):
        if agent_id != self.agent_id:
            return None
        return self.agent_info

    async def get_agent_id_by_subdomain(self, subdomain):
        return self.agent_id

    async def get_published_versions_by_agent_ids(self, agent_ids):
        return {
            agent_id: AgentDisplayInfo(
                name=self.agent_info.name,
                subdomain="fake",
                avatar="",
            )
            for agent_id in agent_ids
        }

    async def get_random_published_agents(self):
        return []
