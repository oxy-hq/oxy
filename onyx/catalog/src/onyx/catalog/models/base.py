from onyx.shared.adapters.orm.schemas import BaseModel
from sqlalchemy import MetaData


class CatalogModel(BaseModel):
    __abstract__ = True
    __table_args__ = {"extend_existing": True}
    metadata = MetaData(
        schema="catalog",
        naming_convention={
            "ix": "ix_%(column_0_label)s",
            "uq": "uq_%(table_name)s_%(column_0_name)s",
            "ck": "ck_%(table_name)s_%(constraint_name)s",
            "fk": "fk_%(table_name)s_%(column_0_name)s_%(referred_table_name)s",
            "pk": "pk_%(table_name)s",
        },
    )
