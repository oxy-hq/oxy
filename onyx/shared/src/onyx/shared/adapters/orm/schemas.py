import json
import uuid
from datetime import datetime, timezone
from enum import StrEnum

from pydantic import ConfigDict
from sqlalchemy import event, func
from sqlalchemy.dialects.postgresql import TIMESTAMP
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column
from sqlalchemy.types import String, TypeDecorator


class StringEnum(TypeDecorator):
    impl = String
    cache_ok = True

    def __init__(self, enum_type: type[StrEnum], *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.enum_type = enum_type

    def process_bind_param(self, value, dialect):
        if isinstance(value, str):
            return self.enum_type(value).value

        if isinstance(value, self.enum_type):
            return value.value

        raise TypeError(f"Invalid type for {self.enum_type}: {value}")

    def process_result_value(self, value, dialect):
        try:
            return self.enum_type(value)
        except ValueError:
            # if the value is not in the enum, return the value as is
            return value


def now_utc():
    return datetime.now(timezone.utc)


def uuid_str():
    return str(uuid.uuid4())


class BaseModel(DeclarativeBase):
    model_config = ConfigDict(
        arbitrary_types_allowed=True,
    )

    id: Mapped[uuid.UUID] = mapped_column(
        default=uuid_str,
        primary_key=True,
    )
    created_at: Mapped[datetime] = mapped_column(TIMESTAMP(), default=now_utc, index=True)
    updated_at: Mapped[datetime] = mapped_column(
        TIMESTAMP(),
        default=now_utc,
        onupdate=func.now(),
        index=True,
    )

    def to_dict(self, exclude: set[str] | None = None):
        if exclude is None:
            exclude = set()
        return {c.name: getattr(self, c.name) for c in self.__table__.columns if c.name not in exclude}

    def to_json(self, exclude: set[str] | None = None):
        return json.dumps(self.to_dict(exclude=exclude))


@event.listens_for(BaseModel, "before_insert", propagate=True)
def set_updated_at_same_as_created_at(mapper, connection, target):
    """
    default_factory give slight difference between created_at and updated_at
    we want to set updated_at to created_at so that we can determine if the model has been changed properly
    """
    target.updated_at = target.created_at = now_utc()
