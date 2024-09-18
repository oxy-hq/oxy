import uuid
from enum import Enum
from typing import TYPE_CHECKING

from onyx.chat.models.base import BaseModel
from sqlalchemy import ForeignKey
from sqlalchemy.dialects import postgresql
from sqlalchemy.orm import Mapped, mapped_column, relationship

if TYPE_CHECKING:
    from onyx.chat.models.message import Message


class FeedbackType(str, Enum):
    positive = "positive"
    negative = "negative"
    neutral = "neutral"

    def __str__(self) -> str:
        return self.value


class Feedback(BaseModel):
    __tablename__ = "feedback"
    content: Mapped[str] = mapped_column(default="")
    user_id: Mapped[uuid.UUID] = mapped_column(index=True)
    message_id: Mapped[str] = mapped_column(ForeignKey("message.id"), index=True)
    feedback_type: Mapped[FeedbackType] = mapped_column(
        postgresql.ENUM(FeedbackType),
        default=FeedbackType.neutral.name,
        server_default=FeedbackType.neutral.name,
    )

    message: Mapped["Message"] = relationship(back_populates="feedbacks")  # noqa: F821
