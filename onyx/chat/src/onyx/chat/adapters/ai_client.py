import abc
from typing import AsyncIterator

from onyx.shared.logging import Logged
from onyx.shared.models.common import AgentInfo, ChatContext, ChatMessage, StreamingChunk, StreamingTrace
from onyx.shared.services.base import Service


class AbstractAIClient(abc.ABC):
    @abc.abstractmethod
    def stream(
        self,
        text: str,
        context: ChatContext,
        chat_history: list[ChatMessage],
        agent_info: AgentInfo,
        cite_sources: bool,
        tracing_session_id: str | None = None,
    ) -> AsyncIterator[StreamingChunk | StreamingTrace]:
        ...


class AIClient(Logged, AbstractAIClient):
    def __init__(self, ai_service: Service):
        self.ai_service = ai_service

    async def stream(
        self,
        text: str,
        context: ChatContext,
        chat_history: list[ChatMessage],
        agent_info: AgentInfo,
        cite_sources: bool,
        tracing_session_id: str | None = None,
    ):
        from onyx.ai.features.agent import StreamRequest

        async for chunk in self.ai_service.handle_generator(
            StreamRequest(
                text=text,
                context=context,
                chat_history=chat_history,
                agent_info=agent_info,
                cite_sources=cite_sources,
                tracing_session_id=tracing_session_id,
            )
        ):
            yield chunk


class FakeAIClient(AbstractAIClient):
    def __init__(self, messages: list[str]):
        self.__messages = messages
        self.__idx = 0

    @property
    def current_message(self):
        return self.__messages[self.__idx]

    def set_idx(self, idx: int):
        if idx < 0 or idx >= len(self.__messages):
            raise ValueError("Invalid index")
        self.__idx = idx

    async def stream(
        self,
        text: str,
        context: ChatContext,
        chat_history: list[ChatMessage],
        agent_info: AgentInfo,
        cite_sources: bool,
        tracing_session_id: str | None = None,
    ) -> AsyncIterator[StreamingChunk | StreamingTrace]:
        for char in self.current_message:
            yield StreamingChunk.content(text=char)
