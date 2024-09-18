from abc import ABC, abstractmethod
from typing import TypedDict

from langchain_core.runnables import Runnable
from onyx.ai.base.citation import CitationMarker
from onyx.shared.models.common import AgentInfo, ChatMessage, DataSource, StreamingChunk, TrainingPrompt


class ChainInput(TypedDict):
    message: str
    username: str
    chat_history: list[ChatMessage]
    agent_info: AgentInfo


class ChainInputWithContext(ChainInput):
    chat_summary: str
    relevant_information: str


class AbstractChainBuilder(ABC):
    @abstractmethod
    def _build(
        self,
        data_sources: list[DataSource],
        training_prompts: list[TrainingPrompt],
        citation_marker: CitationMarker | None,
    ) -> "Runnable[ChainInput, StreamingChunk]":
        ...

    def build(
        self,
        data_sources: list[DataSource],
        training_prompts: list[TrainingPrompt],
        cite_sources: bool = True,
    ) -> "Runnable[ChainInput, StreamingChunk]":
        citation_marker = CitationMarker() if cite_sources else None
        chain = self._build(
            data_sources=data_sources, citation_marker=citation_marker, training_prompts=training_prompts
        )
        return chain
