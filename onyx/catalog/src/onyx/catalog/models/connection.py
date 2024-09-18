import uuid
from typing import TYPE_CHECKING

from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.datasource import DataSourceMixin
from onyx.shared.adapters.orm.schemas import StringEnum
from onyx.shared.models.constants import ConnectionSlugChoices, ConnectionSyncStatus
from onyx.shared.models.utils import MergeMixin
from pydantic import field_validator
from sqlalchemy import JSON, UUID, Boolean, ForeignKey, String
from sqlalchemy.ext.mutable import MutableDict
from sqlalchemy.orm import Mapped, mapped_column, relationship

if TYPE_CHECKING:
    from onyx.catalog.models.namespace import Namespace


def validate_quoted_string(string: str) -> str:
    if string.startswith('"') and string.endswith('"'):
        return string
    return string.lower()


class Connection(CatalogModel, DataSourceMixin, MergeMixin):
    __tablename__: str = "connection"
    __merge_exclude_fields__ = "__all__"
    __merge_children_configs__: tuple[tuple[str, str], ...] = (("tables", "identity"),)

    organization_id: Mapped[uuid.UUID] = mapped_column(UUID, index=True)
    name: Mapped[str] = mapped_column(String)
    slug: Mapped[ConnectionSlugChoices] = mapped_column(String)
    configuration: Mapped[dict[str, str]] = mapped_column(JSON, default={})
    is_system: Mapped[bool] = mapped_column(Boolean, default=False, nullable=False)
    connection_metadata: Mapped[dict | None] = mapped_column(MutableDict.as_mutable(JSON))  # type: ignore
    sync_status: Mapped[ConnectionSyncStatus | None] = mapped_column(StringEnum(ConnectionSyncStatus))
    sync_error: Mapped[str | None] = mapped_column(String, nullable=True)

    namespace_id: Mapped[uuid.UUID | None] = mapped_column(ForeignKey("namespace.id"), nullable=True)
    namespace: Mapped["Namespace"] = relationship("Namespace", foreign_keys=[namespace_id])

    tables: Mapped[list["Table"]] = relationship(
        back_populates="connection",
        cascade="all, delete-orphan",
        passive_deletes=True,
    )

    def to_dict(self, **kwargs):
        return {
            "id": self.id,
            "organization_id": self.organization_id,
            "name": self.name,
            "slug": self.slug,
            "configuration": self.configuration,
            "namespace_id": self.namespace_id,
            "connection_metadata": self.connection_metadata,
            "sync_status": self.sync_status,
            "sync_error": self.sync_error,
        }


class Table(CatalogModel, MergeMixin):
    __tablename__: str = "table"
    __merge_children_configs__: tuple[tuple[str, str], ...] = (("columns", "identity"),)

    organization_id: Mapped[uuid.UUID] = mapped_column(UUID, index=True, nullable=False)
    connection_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("connection.id"))
    columns: Mapped[list["Column"]] = relationship(
        back_populates="table",
        lazy="selectin",
        cascade="all, delete-orphan",
        passive_deletes=True,
    )
    connection: Mapped["Connection"] = relationship(back_populates="tables")

    table_catalog: Mapped[str] = mapped_column(String)
    table_schema: Mapped[str] = mapped_column(String)
    table_name: Mapped[str] = mapped_column(String)
    is_view: Mapped[bool] = mapped_column(Boolean, default=False)
    ddl_query: Mapped[str] = mapped_column(String)

    @property
    def identity(self):
        return self.table_schema, self.table_name

    @field_validator("table_name")
    @classmethod
    def validate_table_name(cls, value):
        return validate_quoted_string(value)


class Column(CatalogModel, MergeMixin):
    __tablename__: str = "columns"
    __merge_exclude_fields__: tuple[str, ...] = (
        "id",
        "created_at",
        "updated_at",
        "table_id",
    )

    organization_id: Mapped[uuid.UUID] = mapped_column(UUID, index=True, nullable=False)
    connection_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("connection.id"))
    table_id: Mapped[uuid.UUID | None] = mapped_column(ForeignKey("table.id"), default=None, nullable=False)
    table: Mapped["Table"] = relationship(back_populates="columns")

    table_catalog: Mapped[str] = mapped_column(String)
    table_schema: Mapped[str] = mapped_column(String)
    table_name: Mapped[str] = mapped_column(String)
    column_name: Mapped[str] = mapped_column(String)
    column_default: Mapped[str | None] = mapped_column(default=None)
    data_type: Mapped[str] = mapped_column(String)
    is_nullable: Mapped[bool] = mapped_column(Boolean)
    is_pk: Mapped[bool | None] = mapped_column(Boolean, default=None)

    @property
    def identity(self):
        return self.table_schema, self.table_name, self.column_name

    @property
    def table_identity(self):
        return self.table_schema, self.table_name

    @field_validator("column_name")
    @classmethod
    def validate_column_name(cls, value):
        return validate_quoted_string(value)
