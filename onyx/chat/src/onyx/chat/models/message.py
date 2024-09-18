import uuid
from enum import Enum
from typing import TYPE_CHECKING

from onyx.chat.models.base import BaseModel
from onyx.shared.models.common import ChatMessage, StreamingChunk
from sqlalchemy import ARRAY, JSON, UUID, ForeignKey, Text
from sqlalchemy.dialects import postgresql
from sqlalchemy.ext.mutable import MutableDict, MutableList
from sqlalchemy.orm import Mapped, mapped_column, relationship
from sqlalchemy.sql import expression

if TYPE_CHECKING:
    from onyx.chat.models.channel import Channel
    from onyx.chat.models.feedback import Feedback


class MessageStatus(str, Enum):
    success = "success"
    streaming = "streaming"
    failure = "failure"

    def __str__(self) -> str:
        return self.value


class Message(BaseModel):
    __tablename__ = "message"
    content: Mapped[str] = mapped_column(Text, default="")
    channel_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("channel.id"), index=True)
    user_id: Mapped[uuid.UUID] = mapped_column(index=True)
    parent_id: Mapped[uuid.UUID | None] = mapped_column(ForeignKey("message.id"), index=True)
    blocks: Mapped[list[dict] | None] = mapped_column(MutableList.as_mutable(ARRAY(JSON)))
    is_ai_message: Mapped[bool] = mapped_column(default=False, server_default=expression.true())

    sources: Mapped[list[dict] | None] = mapped_column(MutableList.as_mutable(ARRAY(JSON)))
    message_metadata: Mapped[dict | None] = mapped_column(MutableDict.as_mutable(JSON))  # type: ignore
    channel: Mapped["Channel"] = relationship(back_populates="messages")  # noqa: F821

    parent: Mapped["Message | None"] = relationship(
        "Message",
        primaryjoin="Message.id == Message.parent_id",
        back_populates="children",
        remote_side="Message.id",
    )
    children: Mapped[list["Message"]] = relationship(back_populates="parent")

    feedbacks: Mapped[list["Feedback"]] = relationship(back_populates="message", cascade="all, delete-orphan")
    trace_id: Mapped[str | None] = mapped_column(UUID)

    status: Mapped[MessageStatus] = mapped_column(
        postgresql.ENUM(MessageStatus),
        default=MessageStatus.success.name,
        server_default=MessageStatus.success.name,
    )

    def to_chat_message(self) -> ChatMessage:
        return ChatMessage(
            content=self.content,
            is_ai_message=self.is_ai_message,
        )

    def apply_streaming_chunk(self, streaming_chunk: StreamingChunk):
        self.content += streaming_chunk.text
        if self.sources is None:
            self.sources = []
        for source in streaming_chunk.sources:
            if source.number not in [s.get("number", None) for s in self.sources]:
                self.sources.append(source.model_dump())

        if self.message_metadata:
            if self.message_metadata.get("steps") is None:
                self.message_metadata["steps"] = []

            self.message_metadata["steps"].extend(streaming_chunk.steps)

    def to_chunk(self, streaming_chunk: StreamingChunk) -> "Message":
        return Message(
            id=self.id,
            content=streaming_chunk.text,
            channel_id=self.channel_id,
            user_id=self.user_id,
            parent_id=self.parent_id,
            is_ai_message=self.is_ai_message,
            sources=self.sources,
            message_metadata=self.message_metadata,
            status=self.status,
        )

    @classmethod
    def user_message(
        cls,
        *,
        user_id: uuid.UUID,
        content: str,
        channel_id: uuid.UUID | None = None,
        parent_id: uuid.UUID | None = None,
    ) -> "Message":
        return cls(
            content=content,
            channel_id=channel_id,  # type: ignore
            user_id=user_id,
            is_ai_message=False,
            sources=[],
            message_metadata={"steps": []},
            status=MessageStatus.success,
            blocks=None,
            parent_id=parent_id,
            trace_id=None,
        )

    @classmethod
    def ai_message_for(cls, user_message: "Message") -> "Message":
        return cls(
            content="",
            channel_id=user_message.channel_id,  # type: ignore
            user_id=user_message.user_id,
            parent_id=user_message.id,
            is_ai_message=True,
            sources=[],
            message_metadata={"steps": []},
            status=MessageStatus.streaming,
        )
