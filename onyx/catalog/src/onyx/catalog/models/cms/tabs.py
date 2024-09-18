from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.cms.agent_category import AgentCategory
from onyx.shared.models.constants import FEATURED_CATEGORY
from sqlalchemy import UUID, Column, ForeignKey, Table
from sqlalchemy.orm import Mapped, mapped_column, relationship

tab_categories_association = Table(
    "tab_categories_association",
    CatalogModel.metadata,
    Column("tab_id", UUID, ForeignKey("discover_tab.id", ondelete="CASCADE"), nullable=False),
    Column("category_id", UUID, ForeignKey("agent_category.id", ondelete="CASCADE"), nullable=False),
)


class DiscoverTab(CatalogModel):
    __tablename__ = "discover_tab"

    name: Mapped[str] = mapped_column(index=True)
    position: Mapped[int] = mapped_column(index=True)
    categories: Mapped[list["AgentCategory"]] = relationship(secondary=tab_categories_association)

    @classmethod
    def featured(cls):
        return DiscoverTab(name=FEATURED_CATEGORY, position=-1)
