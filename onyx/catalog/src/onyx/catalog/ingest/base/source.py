from abc import ABC, abstractmethod
from typing import AsyncContextManager

from onyx.catalog.ingest.base.stream import Stream


class Source(ABC):
    @abstractmethod
    def connect(self) -> AsyncContextManager[list[Stream]]:
        pass
