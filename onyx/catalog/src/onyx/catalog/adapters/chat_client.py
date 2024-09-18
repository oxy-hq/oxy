from abc import ABC, abstractmethod
from uuid import UUID

from onyx.shared.services.base import Service


class AbstractChatClient(ABC):
    @abstractmethod
    async def count_messages(self, agent_ids: list[UUID]) -> dict[UUID, int]:
        pass


class ChatClient(AbstractChatClient):
    def __init__(self, chat_service: Service) -> None:
        self.chat_service = chat_service

    async def count_messages(self, agent_ids: list[UUID]) -> dict[UUID, int]:
        from onyx.chat.features.channel import CountMessageByAgents

        return await self.chat_service.handle(CountMessageByAgents(agent_ids=agent_ids))


class FakeChatClient(AbstractChatClient):
    def __init__(self) -> None:
        self.messages_count: dict[UUID, int] = {}

    def set_messages_count(self, messages_count: dict[UUID, int]) -> None:
        self.messages_count.update(messages_count)

    async def count_messages(self, agent_ids: list[UUID]) -> dict[UUID, int]:
        return {agent_id: self.messages_count.get(agent_id, 0) for agent_id in agent_ids}
