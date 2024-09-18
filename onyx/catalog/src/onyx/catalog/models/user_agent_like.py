import uuid

from onyx.catalog.models.base import CatalogModel
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column


class UserAgentLike(CatalogModel):
    __tablename__: str = "user_agent_like"

    user_id: Mapped[uuid.UUID] = mapped_column(index=True)
    agent_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("agent.id"), index=True)

    def to_dict(self, **kwargs):
        return {"id": self.id, "user_id": self.user_id, "agent_id": self.agent_id}
