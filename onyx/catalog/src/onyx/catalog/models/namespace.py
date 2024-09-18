import uuid

from onyx.catalog.models.base import CatalogModel
from sqlalchemy import UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column


class Namespace(CatalogModel):
    __tablename__ = "namespace"
    __table_args__ = (UniqueConstraint("name", "organization_id", name="_uniq_namespace_organization_id_name"),)

    name: Mapped[str] = mapped_column(index=True)
    organization_id: Mapped[uuid.UUID] = mapped_column(index=True)
    owner_id: Mapped[uuid.UUID | None] = mapped_column(index=True)
