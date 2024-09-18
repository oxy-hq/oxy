from abc import ABC, abstractmethod
from asyncio import Queue, timeout
from contextlib import asynccontextmanager
from typing import Any

from onyx.catalog.ingest.base.context import IngestContext, StreamContext
from onyx.catalog.ingest.base.types import EmbeddingRecord


class Sink(ABC):
    @classmethod
    @asynccontextmanager
    @abstractmethod
    async def connect(cls, context: IngestContext, *args, **kwargs):
        yield cls(*args, **kwargs)

    @abstractmethod
    async def create_schema(self, context: StreamContext):
        pass

    @abstractmethod
    async def sink(self, context: StreamContext, records):
        pass

    def write(self, records):
        if not self.queue:
            # Sink is error out
            raise ValueError("Sink is stopped")
        self.queue.put_nowait(records)

    async def stop(self, wait_for: float = 5 * 60):
        if not self.queue:
            return
        self.queue.put_nowait(None)

        async with timeout(delay=wait_for):
            await self.queue.join()

    async def drain(self, context: StreamContext):
        self.queue = Queue()
        while True:
            records = await self.queue.get()
            if records is None:
                self.queue.task_done()
                break

            try:
                await self.sink(context, records)
                self.queue.task_done()
            except Exception as e:
                self.queue = None
                raise e


class StagingSink(Sink):
    async def sink(self, context, records):
        stg_records = context.to_stg(records)
        await self._sink(context, stg_records)
        await context.update_state(records)

    @abstractmethod
    async def _sink(self, context: StreamContext, records: list[dict[str, Any]]):
        pass


class EmbedSink(Sink):
    async def sink(self, context, records):
        embed_records = await context.to_embed(records)
        await self._sink(context, embed_records)

    @abstractmethod
    async def _sink(self, context: StreamContext, records: list[EmbeddingRecord]):
        pass
