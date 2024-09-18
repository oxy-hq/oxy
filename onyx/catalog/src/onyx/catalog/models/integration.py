import uuid
from typing import TYPE_CHECKING

from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.datasource import DataSourceMixin
from onyx.catalog.models.prompt_integration import PromptIntegration
from onyx.shared.adapters.orm.schemas import StringEnum
from onyx.shared.models.constants import IntegrationSlugChoices
from onyx.shared.models.utils import canonicalize
from sqlalchemy import JSON, ForeignKey
from sqlalchemy.ext.mutable import MutableDict
from sqlalchemy.orm import Mapped, mapped_column, relationship

if TYPE_CHECKING:
    from onyx.catalog.models.ingest_state import IngestState
    from onyx.catalog.models.namespace import Namespace
    from onyx.catalog.models.prompt import Prompt
    from onyx.catalog.models.task import Task


class Integration(CatalogModel, DataSourceMixin):
    __tablename__: str = "integration"

    organization_id: Mapped[uuid.UUID] = mapped_column(index=True)
    name: Mapped[str] = mapped_column()
    slug: Mapped[IntegrationSlugChoices] = mapped_column(StringEnum(IntegrationSlugChoices))
    configuration: Mapped[dict] = mapped_column(JSON, default={})
    integration_metadata: Mapped[dict | None] = mapped_column(MutableDict.as_mutable(JSON))  # type: ignore

    namespace_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("namespace.id"))
    namespace: Mapped["Namespace"] = relationship("Namespace")
    tasks: Mapped[list["Task"]] = relationship("Task")
    prompts: Mapped[list["Prompt"]] = relationship("Prompt", secondary=PromptIntegration.__table__)
    ingest_state: Mapped["IngestState | None"] = relationship("IngestState", uselist=False)

    @property
    def target_stg_schema(self):
        return canonicalize(f"stg__{self.organization_id}__{self.id}")

    @property
    def target_prod_schema(self):
        return canonicalize(f"prod__{self.organization_id}__{self.id}")

    def to_dict(self, **kwargs):
        return {
            "id": self.id,
            "organization_id": self.organization_id,
            "name": self.name,
            "slug": self.slug,
            "configuration": self.configuration,
            "integration_metadata": self.integration_metadata,
            "namespace_id": self.namespace_id,
        }
