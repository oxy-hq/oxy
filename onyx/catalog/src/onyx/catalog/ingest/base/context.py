from asyncio import gather
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Iterable

from onyx.catalog.ingest.base.processor import ProcessingStrategy
from onyx.catalog.ingest.base.storage import Storage
from onyx.catalog.ingest.base.types import Identity, Interval, Record
from onyx.catalog.ingest.base.utils import merge_overlap


@dataclass
class IngestRequest:
    identity: Identity
    request_interval: Interval | None = None
    default_beginning_delta: timedelta = timedelta(days=30)


@dataclass
class IngestContext:
    identity: Identity
    request_interval: Interval
    batch_size: int = 100
    rewrite: bool = False


@dataclass
class StreamContext:
    name: str
    ingest_context: IngestContext
    properties: list[tuple[str, str]]
    key_properties: list[str]
    bookmark_property: str
    embedding_strategy: ProcessingStrategy
    state_storage: Storage
    current_intervals: list[Interval] = field(default_factory=list)

    @property
    def identity(self):
        return self.ingest_context.identity

    @property
    def request_interval(self):
        return self.ingest_context.request_interval

    @property
    def batch_size(self):
        return self.ingest_context.batch_size

    @property
    def rewrite(self):
        return self.ingest_context.rewrite

    @property
    def stg_table_name(self):
        return self.identity.staging_table(self.name)

    def to_stg(self, records: Iterable[Record]):
        return [record.model_dump() for record in records]

    async def to_embed(self, records: Iterable[Record]):
        coros = [self.embedding_strategy.process_record(record) for record in records]
        return await gather(*coros)

    async def update_state(self, records: Iterable[Record]):
        def mapper(record: Record):
            ts = self.__serialize_ts(getattr(record, self.bookmark_property))
            return ts

        timestamps = list(map(mapper, records))
        if not timestamps:
            return

        min_ts = min(timestamps)
        max_ts = max(timestamps)

        interval = Interval(min_ts, max_ts)
        self.current_intervals.append(interval)
        merge_overlap(self.current_intervals)
        await self.state_storage.write_stream_state(self.identity, self.name, self.current_intervals)

    def __serialize_ts(self, ts: int | datetime | float):
        if isinstance(ts, datetime):
            return int(ts.timestamp())
        return int(ts)
