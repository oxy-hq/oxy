from datetime import datetime
from uuid import UUID

from onyx.catalog.models.base import CatalogModel
from onyx.shared.adapters.orm.schemas import StringEnum
from onyx.shared.models.constants import ConnectionSyncStatus
from sqlalchemy import JSON, ForeignKey, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column


class IngestState(CatalogModel):
    __tablename__ = "ingest_state"
    __table_args__ = (UniqueConstraint("integration_id"),)

    integration_id: Mapped[UUID] = mapped_column(
        ForeignKey("integration.id", ondelete="CASCADE", name="fk_ingest_state_integration_id_integration"), index=True
    )
    bookmarks: Mapped[dict] = mapped_column(JSON, default={})
    sync_status: Mapped[ConnectionSyncStatus] = mapped_column(
        StringEnum(ConnectionSyncStatus), default=ConnectionSyncStatus.initial
    )
    sync_error: Mapped[str | None] = mapped_column()
    last_synced_at: Mapped[datetime | None] = mapped_column()
    last_success_bookmark: Mapped[datetime | None] = mapped_column()
