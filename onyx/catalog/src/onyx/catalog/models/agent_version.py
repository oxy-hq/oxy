import uuid
from typing import TYPE_CHECKING, cast

from onyx.catalog.models.agent_version_integration import AgentVersionConnection, AgentVersionIntegration
from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.datasource import DataSourceModel
from onyx.shared.models.common import AgentInfo, DataSource
from onyx.shared.models.constants import DataSourceType
from sqlalchemy import ARRAY, JSON, ForeignKey, String, Text
from sqlalchemy.ext.mutable import MutableDict
from sqlalchemy.orm import Mapped, mapped_column, relationship

if TYPE_CHECKING:
    from onyx.catalog.models.agent import Agent
    from onyx.catalog.models.connection import Connection
    from onyx.catalog.models.integration import Integration
    from onyx.catalog.models.prompt import Prompt


class AgentVersion(CatalogModel):
    __tablename__ = "agent_version"
    agent_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("agent.id", ondelete="CASCADE"))
    agent: Mapped["Agent"] = relationship(
        back_populates="versions",
        foreign_keys="[AgentVersion.agent_id]",
    )
    name: Mapped[str] = mapped_column(default="")
    instructions: Mapped[str] = mapped_column(Text, default="")
    description: Mapped[str] = mapped_column(Text, default="")
    avatar: Mapped[str] = mapped_column(default="")
    greeting: Mapped[str] = mapped_column(default="")
    subdomain: Mapped[str] = mapped_column(default="")
    knowledge: Mapped[str] = mapped_column(Text, default="")
    starters: Mapped[list[str]] = mapped_column(ARRAY(String), default=[])
    is_published: Mapped[bool] = mapped_column(default=False)
    agent_metadata: Mapped[dict | None] = mapped_column(MutableDict.as_mutable(JSON))  # type: ignore
    prompts: Mapped[list["Prompt"]] = relationship(
        back_populates="agent_version",
        cascade="delete",
    )

    integrations: Mapped[list["Integration"]] = relationship(
        "Integration",
        secondary=AgentVersionIntegration.__table__,
        cascade="expunge",
    )
    connections: Mapped[list["Connection"]] = relationship(
        "Connection",
        secondary=AgentVersionConnection.__table__,
        cascade="expunge",
    )

    @property
    def data_sources(self, dev=False) -> list[DataSource]:
        results: list[DataSource] = []
        for integration in self.integrations:
            results.append(
                DataSourceModel(
                    organization_id=integration.organization_id,
                    id=cast(uuid.UUID, integration.id),
                    slug=integration.slug,
                    name=integration.name,
                    type=DataSourceType.integration,
                    schema=None,
                    metadata=integration.integration_metadata or {},
                ).to_dict()
            )
        for connection in self.connections:
            results.append(
                DataSourceModel(
                    organization_id=connection.organization_id,
                    id=cast(uuid.UUID, connection.id),
                    slug=connection.slug,
                    name=connection.name,
                    type=DataSourceType.warehouse,
                    schema=connection.tables,
                    metadata=connection.connection_metadata or {},
                ).to_dict()
            )
        return results

    def clone(self):
        version = AgentVersion(
            agent_id=self.agent_id,
            name=self.name,
            instructions=self.instructions,
            description=self.description,
            avatar=self.avatar,
            greeting=self.greeting,
            subdomain=self.subdomain,
            knowledge=self.knowledge,
            starters=self.starters,
            is_published=self.is_published,
            integrations=self.integrations,
            connections=self.connections,
            agent_metadata=self.agent_metadata,
        )

        for prompt in self.prompts:
            cloned_prompt = prompt.clone()
            cloned_prompt.agent_version_id = version.id
            version.prompts.append(cloned_prompt)

        return version

    @property
    def is_changed(self):
        # there is no published version
        if self.agent.published_version_id is None:
            return True

        if self.agent.published_version_id == self.id:
            return False

        # if the version changed
        if self.updated_at > self.created_at:
            return True

        # if any prompt changed
        for prompt in self.prompts:
            if prompt.is_changed:
                return True
        return False

    def to_dict(self):
        rs = {
            "id": str(self.id),
            "name": self.name,
            "instructions": self.instructions,
            "description": self.description,
            "avatar": self.avatar,
            "greeting": self.greeting,
            "subdomain": self.subdomain,
            "knowledge": self.knowledge,
            "starters": self.starters,
            "is_published": self.is_published,
            "agent_metadata": self.agent_metadata,
            "is_changed": self.is_changed,
            "integrations": [integration.to_dict() for integration in self.integrations],
            "connections": [connection.to_dict() for connection in self.connections],
        }
        return rs

    def to_info(self) -> AgentInfo:
        return AgentInfo(
            data_sources=self.data_sources,
            description=self.description,
            instructions=self.instructions,
            name=self.name,
            training_prompts=[prompt.to_training_prompt() for prompt in self.prompts],
            knowledge=self.knowledge,
        )
