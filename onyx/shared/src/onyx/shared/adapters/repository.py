import abc
from typing import Generic, Iterable, TypeVar
from uuid import UUID

ModelType = TypeVar("ModelType")


class GenericRepository(Generic[ModelType], abc.ABC):
    @abc.abstractmethod
    def get_by_id(self, id: str | UUID | int) -> ModelType | None:
        ...

    @abc.abstractmethod
    def get_for_update(self, id: str | UUID | int) -> ModelType | None:
        ...

    @abc.abstractmethod
    def list(self, **filters) -> Iterable[ModelType]:
        ...

    @abc.abstractmethod
    def add(self, record: ModelType) -> ModelType:
        ...

    @abc.abstractmethod
    def delete(self, id: str) -> None:
        ...
