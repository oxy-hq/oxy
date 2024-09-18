from abc import ABC, abstractmethod
from typing import Callable, TypeVar

from onyx.shared.adapters.orm.database import SessionFactoryFunc
from onyx.shared.services.unit_of_work import AbstractUnitOfWork
from sqlalchemy.orm import Session


class SQLUnitOfWorkMixin(AbstractUnitOfWork, ABC):
    def __init__(self, session: Session) -> None:
        self._session = session
        self._init_repositories(self._session)
        super().__init__()

    def __exit__(self, *args):
        super().__exit__(*args)
        self._session.close()

    def commit(self):
        self._session.commit()

    def rollback(self):
        self._session.rollback()

    @abstractmethod
    def _init_repositories(self, session: Session):
        ...


UOWType = TypeVar("UOWType", bound=SQLUnitOfWorkMixin)


def sql_uow_factory(session_factory: SessionFactoryFunc, cls: type[UOWType]) -> Callable[[], UOWType]:
    def factory():
        return cls(session_factory())

    return factory
