import uuid

from onyx.catalog.models.base import CatalogModel
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column


class AgentVersionIntegration(CatalogModel):
    __tablename__ = "agent_version_integration"
    agent_version_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("agent_version.id"), index=True)
    integration_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("integration.id"), index=True)
    is_deleted: Mapped[bool] = mapped_column(default=False)


class AgentVersionConnection(CatalogModel):
    __tablename__ = "agent_version_connection"
    agent_version_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("agent_version.id"), index=True)
    connection_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("connection.id"), index=True)
    is_deleted: Mapped[bool] = mapped_column(default=False)
