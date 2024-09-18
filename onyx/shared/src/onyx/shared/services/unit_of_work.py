from abc import ABC, abstractmethod


class AbstractUnitOfWork(ABC):
    @abstractmethod
    def commit(self):
        ...

    @abstractmethod
    def rollback(self):
        ...

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.rollback()
