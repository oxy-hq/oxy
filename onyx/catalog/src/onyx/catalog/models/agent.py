import uuid
from dataclasses import dataclass
from typing import TYPE_CHECKING

from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.cms.agent_category import AgentCategory
from onyx.shared.models.common import AgentInfo
from sqlalchemy import UUID, Column, ForeignKey, Table
from sqlalchemy.orm import Mapped, mapped_column, relationship

if TYPE_CHECKING:
    from onyx.catalog.models.agent_version import AgentVersion

agent_categories_association = Table(
    "agent_categories_association",
    CatalogModel.metadata,
    Column("agent_id", UUID, ForeignKey("agent.id"), nullable=False),
    Column("category_id", UUID, ForeignKey("agent_category.id"), nullable=False),
)


@dataclass
class SearchAgent:
    id: uuid.UUID
    name: str
    description: str
    avatar: str
    subdomain: str
    reason: str
    total_likes: int
    total_messages: int

    def to_dict(self):
        return {
            "id": str(self.id),
            "name": self.name,
            "description": self.description,
            "avatar": self.avatar,
            "subdomain": self.subdomain,
            "reason": self.reason,
            "total_likes": self.total_likes,
            "total_messages": self.total_messages,
        }


class Agent(CatalogModel):
    __tablename__ = "agent"

    organization_id: Mapped[uuid.UUID] = mapped_column(index=True)
    is_deleted: Mapped[bool] = mapped_column(default=False)
    is_featured: Mapped[bool] = mapped_column(default=False)
    weight: Mapped[int] = mapped_column(default=0)

    published_version_id: Mapped[uuid.UUID | None] = mapped_column(
        ForeignKey("agent_version.id", name="fk_agent_published_version_id_agent_version", ondelete="SET NULL")
    )
    published_version: Mapped["AgentVersion"] = relationship(foreign_keys=[published_version_id])
    dev_version_id: Mapped[uuid.UUID | None] = mapped_column(
        ForeignKey("agent_version.id", name="fk_agent_dev_version_id_agent_version", ondelete="SET NULL")
    )
    dev_version: Mapped["AgentVersion"] = relationship(foreign_keys=[dev_version_id])

    versions: Mapped[list["AgentVersion"]] = relationship(
        back_populates="agent", foreign_keys="[AgentVersion.agent_id]"
    )
    categories: Mapped[list["AgentCategory"]] = relationship(secondary=agent_categories_association)

    def featured(self, position: int):
        self.is_featured = True
        self.weight = position

    def unfeatured(self):
        self.is_featured = False
        self.weight = 0

    def to_info(self, published: bool) -> AgentInfo | None:
        version = None
        if published:
            version = self.published_version
        else:
            version = self.dev_version

        if not version:
            return None

        return version.to_info()

    def to_published_dict(self):
        if not self.published_version:
            return None
        return {
            "id": str(self.id),
            "weight": self.weight,
            "featured": self.is_featured,
            "name": self.published_version.name,
            "description": self.published_version.description,
            "avatar": self.published_version.avatar,
            "organization_id": str(self.organization_id),
            "subdomain": self.published_version.subdomain,
            "agent_metadata": self.published_version.agent_metadata,
            "starters": self.published_version.starters,
            "greeting": self.published_version.greeting,
            "is_deleted": self.is_deleted,
            "instructions": self.published_version.instructions,
            "knowledge": self.published_version.knowledge,
            "integrations": [integration.to_dict() for integration in self.published_version.integrations],
            "updated_at": self.published_version.updated_at,
        }

    def to_dict(self):
        return {
            "id": str(self.id),
            "weight": self.weight,
            "featured": self.is_featured,
            "organization_id": str(self.organization_id),
            "published_version": self.published_version.to_dict() if self.published_version else None,
            "dev_version": self.dev_version.to_dict() if self.dev_version else None,
        }

    def to_dev_dict(self):
        return self.to_dict()

    @property
    def have_dev_version(self):
        return self.dev_version is not None and self.dev_version.is_published is False
