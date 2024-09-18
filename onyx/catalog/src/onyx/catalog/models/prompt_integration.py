import uuid

from onyx.catalog.models.base import CatalogModel
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column


class PromptIntegration(CatalogModel):
    __tablename__ = "prompt_integration"
    prompt_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("prompt.id"), index=True)
    integration_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("integration.id"), index=True)
    is_deleted: Mapped[bool] = mapped_column(
        default=False,
    )
