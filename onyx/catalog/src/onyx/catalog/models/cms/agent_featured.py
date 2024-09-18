from uuid import UUID

from onyx.catalog.models.base import CatalogModel
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column


class AgentFeatured(CatalogModel):
    __tablename__ = "agent_featured"

    agent_id: Mapped[UUID] = mapped_column(ForeignKey("agent.id", ondelete="CASCADE"))
    position: Mapped[int] = mapped_column(index=True)
