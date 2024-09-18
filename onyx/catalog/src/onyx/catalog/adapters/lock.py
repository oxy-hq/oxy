from abc import ABC, abstractmethod
from threading import Lock as BaseThreadLock
from typing import Callable, Self
from weakref import WeakValueDictionary

from onyx.shared.adapters.orm.errors import RowLockedError


class AbstractLock(ABC):
    def __init__(self, *, key: str, timeout: float) -> None:
        self.key = key
        self.timeout = timeout

    @abstractmethod
    def acquire(self) -> bool:
        ...

    @abstractmethod
    def release(self) -> None:
        ...

    @classmethod
    def factory(cls, **kwargs) -> "Callable[[str, float], Self]":
        def create(key: str, timeout: float):
            return cls(key=key, timeout=timeout, **kwargs)

        return create

    def __enter__(self) -> "Self":
        acquired = self.acquire()
        if acquired:
            return self
        raise RowLockedError

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        self.release()


LockFactory = Callable[[str, float], AbstractLock]


class ThreadLock(AbstractLock):
    registry = WeakValueDictionary[str, BaseThreadLock]()
    lock = BaseThreadLock()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.__lock: BaseThreadLock | None = None

    def acquire(self) -> bool:
        self.__lock = self.get_lock(self.key)
        return self.__lock.acquire(blocking=False)

    def release(self) -> None:
        if self.__lock:
            self.__lock.release()
            self.__lock = None

    @classmethod
    def get_lock(cls, key: str) -> BaseThreadLock:
        with cls.lock:
            if key not in cls.registry:
                lock = BaseThreadLock()
                cls.registry[key] = lock
            return cls.registry[key]
