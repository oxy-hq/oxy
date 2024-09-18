from onyx.catalog.models.base import CatalogModel
from onyx.shared.models.constants import FEATURED_CATEGORY
from pydantic import BaseModel
from sqlalchemy.orm import Mapped, mapped_column


class Category(BaseModel):
    label: str
    value: str

    @classmethod
    def featured(cls) -> "Category":
        return cls(label="Featured", value=FEATURED_CATEGORY)


class AgentCategory(CatalogModel):
    __tablename__ = "agent_category"

    label: Mapped[str] = mapped_column()
    value: Mapped[str] = mapped_column()

    def to_category(self) -> Category:
        return Category(label=self.label, value=self.value)
