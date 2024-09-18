from abc import ABC, abstractmethod
from uuid import UUID

from onyx.chat.models.channel import Channel
from onyx.chat.models.message import Message
from onyx.shared.adapters.orm.repository import GenericSqlRepository
from onyx.shared.adapters.repository import GenericRepository
from onyx.shared.models.common import ChatMessage
from sqlalchemy import false, select
from sqlalchemy.orm import Session


class AbstractChannelRepository(GenericRepository[Channel], ABC):
    @abstractmethod
    def get_active_agent_channel(self, agent_id: UUID, created_by: UUID) -> Channel | None:
        ...


class ChannelRepository(GenericSqlRepository[Channel], AbstractChannelRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Channel)

    def get_active_agent_channel(self, agent_id: UUID, created_by: UUID) -> Channel | None:
        stmt = (
            select(Channel)
            .where(Channel.agent_id == agent_id)
            .where(Channel.created_by == created_by)
            .where(Channel.is_deleted == false())
        )
        channel = self._session.scalars(stmt).first()
        return channel


class AbstractMessageRepository(GenericRepository[Message], ABC):
    @abstractmethod
    def list_messages(self, channel_id: UUID, limit: int = 10) -> list[ChatMessage]:
        ...


class MessageRepository(GenericSqlRepository[Message], AbstractMessageRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Message)

    def list_messages(self, channel_id: UUID, limit: int = 10) -> list[ChatMessage]:
        stmt = select(Message).filter(Message.channel_id == channel_id).order_by(Message.created_at.desc()).limit(limit)
        messages = self._session.scalars(stmt).all()
        results = [message.to_chat_message() for message in messages]
        results.reverse()
        return results
