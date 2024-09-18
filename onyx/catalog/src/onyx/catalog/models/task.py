import uuid

from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.integration import Integration
from onyx.shared.models.constants import TaskQueueSystems
from sqlalchemy import JSON, Enum, ForeignKey, String
from sqlalchemy.orm import Mapped, mapped_column, relationship


class SourceType(str, Enum):
    integration = "integration"
    connection = "connection"


class ExecutionType(str, Enum):
    manual = "manual"
    schedule = "schedule"


class Task(CatalogModel):
    __tablename__: str = "task"
    external_id: Mapped[str] = mapped_column()
    queue_system: Mapped[TaskQueueSystems] = mapped_column(String)
    request_payload: Mapped[dict] = mapped_column(JSON, default={})

    source_type: Mapped[SourceType | None] = mapped_column(String)
    source_id: Mapped[uuid.UUID | None] = mapped_column(ForeignKey("integration.id"))

    execution_type: Mapped[ExecutionType | None] = mapped_column(String)

    integration: Mapped[Integration] = relationship("Integration", back_populates="tasks")
