from abc import ABC, abstractmethod
from datetime import datetime, timedelta

from onyx.catalog.ingest.base.types import Identity, Interval


class Storage(ABC):
    @abstractmethod
    async def read_stream_state(self, identity: Identity, stream_name: str) -> list[Interval]:
        pass

    @abstractmethod
    async def write_stream_state(self, identity: Identity, stream_name: str, intervals: list[Interval]):
        pass

    @abstractmethod
    async def write_source_state(
        self, identity: Identity, last_success_bookmark: datetime | None = None, error: str | None = None
    ):
        pass

    @abstractmethod
    async def generate_request_interval(self, identity: Identity, default_beginning_delta: timedelta) -> Interval:
        pass
