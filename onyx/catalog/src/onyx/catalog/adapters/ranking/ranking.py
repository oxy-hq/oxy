from abc import ABC, abstractmethod

from dspy import InputField, OutputField, Signature, settings
from onyx.catalog.adapters.ranking.openai import GPTRanking
from onyx.catalog.adapters.ranking.predict import SCORE_FIELD, RankingPredict
from onyx.catalog.models.search import AgentDocument
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from onyx.shared.services.dispatcher import AbstractDispatcher


class AgentRankingSignature(Signature):
    """You are an Assistant responsible for helping detect whether the retrieved person is relevant to the query.
    Consider partial names match as relevant.
    For example:
    - if the query is "John Doe" and the retrieved person is "John Smith" or "Jane Doe", consider it relevant.
    - if the query is "John" and the retrieved person is "John Smith", consider it irrelevant.
    - if the query is "Smith" and the retrieved person is "John Smith", consider it irrelevant.
    """

    query = InputField(desc="The query to search for.")
    retrieved_person = InputField(desc="The person information retrieved from the search.")
    is_relevant = OutputField(desc="Yes or No")
    reason = OutputField(
        desc='The reason for your Yes/No decision in following format w/o mentioning the person name: "Talks about <topic a>, <topic b>, <topic c>".'
    )


class AbstractRankingAdapter(ABC):
    @abstractmethod
    async def rerank(self, query: str, agents: list[AgentDocument]) -> list[AgentDocument]:
        ...


class GPTRankingAdapter(Logged, AbstractRankingAdapter):
    def __init__(self, config: OnyxConfig, dispatcher: AbstractDispatcher):
        self.dispatcher = dispatcher
        self.rerank_lm = GPTRanking(model=config.openai.chat_model, max_tokens=250, api_key=config.openai.api_key)
        self.rerank_predict = RankingPredict(AgentRankingSignature)
        self.threshold = config.search.ranking_threshold

    async def __rerank_agents(self, agents: list[AgentDocument], query: str, threshold: float) -> list[AgentDocument]:
        with settings.context(lm=self.rerank_lm):
            response = await self.dispatcher.map(
                self.rerank_predict,
                [
                    {
                        "args": [],
                        "kwargs": {
                            "query": query,
                            "retrieved_person": agent.to_document(),
                            "config": {
                                "ranking_on_field": "is_relevant",
                            },
                        },
                    }
                    for agent in agents
                ],
            )
            self.log.info(f"Rerank Response: {response}")
            relevant_agents = []
            for idx, agent in enumerate(agents):
                rerank_response = None
                try:
                    rerank_response = response[idx]
                except IndexError:
                    pass
                if rerank_response is None:
                    relevant_agents.append(agent)
                    continue

                score = rerank_response.get(SCORE_FIELD, 0.0)
                is_relevant = rerank_response.get("is_relevant", "").lower() == "yes"
                # Normalize yes/no score
                if not is_relevant:
                    score = 1 - score

                if score < threshold:
                    continue

                agent.relevance = score
                agent.reason = rerank_response.get("reason", "")
                relevant_agents.append(agent)

            sorted_relevants = sorted(relevant_agents, key=lambda x: x.relevance, reverse=True)
            self.log.info(f"Agents: {agents}")
            self.log.info(f"Relevant Agents: {sorted_relevants}")

            return sorted_relevants

    async def rerank(self, query: str, agents: list[AgentDocument]) -> list[AgentDocument]:
        return await self.__rerank_agents(agents, query, self.threshold)
