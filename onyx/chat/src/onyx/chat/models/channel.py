import uuid
from datetime import datetime, timezone
from typing import TYPE_CHECKING

from onyx.chat.models.base import BaseModel
from sqlalchemy.dialects.postgresql import TIMESTAMP
from sqlalchemy.orm import Mapped, mapped_column, relationship

if TYPE_CHECKING:
    from onyx.chat.models.message import Message


def now_utc():
    return datetime.now(timezone.utc)


class Channel(BaseModel):
    __tablename__ = "channel"
    name: Mapped[str] = mapped_column(default="")
    organization_id: Mapped[uuid.UUID] = mapped_column(index=True)
    created_by: Mapped[uuid.UUID] = mapped_column(index=True)
    is_deleted: Mapped[bool] = mapped_column(default=False)
    is_public: Mapped[bool] = mapped_column(default=False)
    agent_id: Mapped[uuid.UUID | None] = mapped_column(
        index=True,
    )
    last_message_at: Mapped[datetime] = mapped_column(TIMESTAMP(), default=now_utc)

    messages: Mapped[list["Message"]] = relationship(back_populates="channel", cascade="all, delete-orphan")  # noqa: F821
