import uuid
from dataclasses import dataclass
from typing import TYPE_CHECKING

from onyx.shared.models.constants import ConnectionSlugChoices
from onyx.shared.models.utils import canonicalize

if TYPE_CHECKING:
    from onyx.catalog.models.connection import Table
    from onyx.shared.models.common import DataSource
    from onyx.shared.models.constants import (
        DataSourceType,
        IntegrationSlugChoices,
    )


class DataSourceMixin:
    organization_id: uuid.UUID
    slug: "IntegrationSlugChoices"
    id: uuid.UUID

    @property
    def target_embedding_schema(self):
        return canonicalize(f"embed__{self.organization_id}")

    @property
    def target_embedding_table(self):
        return canonicalize(f"{self.slug}__{self.id}")


@dataclass
class DataSourceModel(DataSourceMixin):
    name: str
    organization_id: uuid.UUID
    slug: "IntegrationSlugChoices | ConnectionSlugChoices"
    id: uuid.UUID
    type: "DataSourceType"
    schema: list["Table"] | None
    metadata: dict

    def to_dict(self) -> "DataSource":
        return {
            "slug": self.slug,
            "name": self.name,
            "database": self.target_embedding_schema,
            "table": self.target_embedding_table,
            "type": self.type,
            "organization_id": self.organization_id,
            "id": self.id,
            "metadata": self.metadata,
            "source_tables": [
                {
                    "schema": table.table_schema,
                    "name": table.table_name,
                    "columns": [
                        {
                            "name": column.column_name,
                            "type": column.data_type,
                        }
                        for column in table.columns
                    ],
                }
                for table in self.schema
            ]
            if self.schema
            else [],
        }
