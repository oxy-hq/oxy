from abc import ABC, abstractmethod
from uuid import UUID

from onyx.catalog.adapters.ranking.ranking import AbstractRankingAdapter
from onyx.catalog.models.search import AgentDocument
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from openai import AsyncOpenAI

from vespa.application import Vespa


class AbstractSearchClient(ABC):
    question_dict = {"who", "what", "where", "when", "why", "how", "which", "whom", "whose", "whether"}
    question_mark = "?"

    @abstractmethod
    async def search_agents(self, query: str, limit: int = 6) -> list[AgentDocument]:
        pass

    @abstractmethod
    async def index_agent(self, agent: AgentDocument) -> bool:
        pass

    @abstractmethod
    async def delete_agent(self, agent_id: UUID) -> bool:
        pass

    def is_search_agent(self, query: str) -> bool:
        if self.question_mark in query:
            return False

        for word in query.lower().split(" "):
            if word in self.question_dict:
                return False

        return True


class VespaSearchClient(Logged, AbstractSearchClient):
    namespace: str = "public"

    def __init__(self, config: OnyxConfig, rank: AbstractRankingAdapter):
        self.rank = rank
        self.vespa = Vespa(url=config.vespa.url, vespa_cloud_secret_token=config.vespa.cloud_secret_token)
        self.embedder = AsyncOpenAI(api_key=config.openai.api_key).embeddings
        self.model = config.openai.embeddings_model
        self.schema = config.vespa.agent_document_type
        self.groupname = config.vespa.agent_document_type

    async def search_agents(self, query: str, limit: int = 6) -> list[AgentDocument]:
        query_embedding = await self.__embed_query(query)
        async with self.vespa.asyncio() as client:
            response = await client.query(
                **{
                    "yql": f"select * from {self.schema} where (userQuery() or ({{targetHits:100}}nearestNeighbor(embeddings,q)))",
                    "ranking": "hybrid",
                    "body": {"input.query(q)": query_embedding},
                    "query": query,
                    "hits": limit,
                }
            )

            if response.is_successful():
                agents = [AgentDocument.from_vespa(doc) for doc in response.hits]
                self.log.info(f"Search Agents Response: {[(agent.name, agent.relevance) for agent in agents]}")
                return await self.rank.rerank(query, agents)
            else:
                self.log.error(f"Search Agents Response: {response.json}")
                return []

    async def __embed_query(self, query: str) -> list[float]:
        response = await self.embedder.create(input=query, model=self.model)
        return response.data[0].embedding

    async def __embed(self, texts: list[str]) -> dict[str, list[float]]:
        response = await self.embedder.create(input=texts, model=self.model)
        return {str(entry.index): entry.embedding for entry in response.data}

    async def index_agent(self, agent: AgentDocument) -> bool:
        async with self.vespa.asyncio() as client:
            response = await client.update_data(
                namespace=self.namespace,
                groupname=self.groupname,
                schema=self.schema,
                data_id=str(agent.id),
                fields={
                    **agent.to_vespa_fields(),
                    "embeddings": await self.__embed([agent.description, *agent.conversation_starters]),  # type: ignore
                },
                create=True,
            )
            self.log.info(f"Index Agent Response: {response.json}")
            return response.is_successful()

    async def delete_agent(self, agent_id: UUID) -> bool:
        async with self.vespa.asyncio() as client:
            response = await client.delete_data(
                namespace=self.namespace, groupname=self.groupname, schema=self.schema, data_id=str(agent_id)
            )
            self.log.info(f"Delete Agent Response: {response.json}")
            return response.is_successful()


class FakeSearchClient(Logged, AbstractSearchClient):
    def __init__(self) -> None:
        self.agents: dict[str, AgentDocument] = {}

    async def search_agents(self, query: str, limit: int = 6) -> list[AgentDocument]:
        return [agent for agent in self.agents.values() if query in agent.name or query in agent.description]

    async def index_agent(self, agent: AgentDocument) -> bool:
        self.log.info(f"Indexing Agent: {agent}")
        self.agents[str(agent.id)] = agent
        return True

    async def delete_agent(self, agent_id: UUID) -> bool:
        del self.agents[str(agent_id)]
        return True
