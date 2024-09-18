from typing import Iterable, TypeVar
from uuid import UUID

from onyx.shared.adapters.orm.errors import ORMInvalidColumnError, RowLockedError
from onyx.shared.adapters.orm.schemas import BaseModel
from onyx.shared.adapters.repository import GenericRepository
from psycopg2.errors import LockNotAvailable
from sqlalchemy import Select, and_, select
from sqlalchemy.exc import OperationalError
from sqlalchemy.orm import Session

ModelType = TypeVar("ModelType", bound=BaseModel)


class GenericSqlRepository(GenericRepository[ModelType]):
    def __init__(self, session: Session, model_cls: type[ModelType]) -> None:
        self._session = session
        self._model_cls = model_cls

    def _construct_get_stmt(self, id: str) -> Select:
        stmt = select(self._model_cls).where(self._model_cls.id == id)
        return stmt

    def get_by_id(self, id: str) -> ModelType | None:
        stmt = self._construct_get_stmt(id)
        return self._session.execute(stmt).scalar_one_or_none()

    def get_for_update(self, id: str | UUID | int) -> ModelType | None:
        try:
            stmt = self._construct_get_stmt(id)  # type: ignore
            stmt = stmt.with_for_update(nowait=True)
            return self._session.execute(stmt).scalar_one_or_none()
        except OperationalError as e:
            if e.orig is LockNotAvailable:
                raise RowLockedError(f"Row with id {id} is locked for update") from e
            raise e

    def _construct_list_stmt(self, **filters) -> Select:
        stmt = select(self._model_cls)
        where_clauses = []
        for c, v in filters.items():
            if not hasattr(self._model_cls, c):
                raise ORMInvalidColumnError(f"Invalid column name {c}")
            where_clauses.append(getattr(self._model_cls, c) == v)

        if where_clauses:
            stmt = stmt.where(and_(*where_clauses))
        return stmt

    def list(self, **filters) -> Iterable[ModelType]:
        stmt = self._construct_list_stmt(**filters)
        return self._session.execute(stmt).scalars().all()

    def add(self, record: ModelType) -> ModelType:
        self._session.add(record)
        self._session.flush()
        self._session.refresh(record)
        return record

    def delete(self, id: str) -> None:
        record = self.get_by_id(id)
        if record is not None:
            self._session.delete(record)
            self._session.flush()
