from asyncio import CancelledError, create_task, gather
from contextlib import AsyncExitStack
from datetime import datetime
from typing import cast

from onyx.catalog.ingest.adapters.ibis import IbisSink
from onyx.catalog.ingest.adapters.storage import IntegrationStateStorage
from onyx.catalog.ingest.adapters.vespa import VespaSink
from onyx.catalog.ingest.base.context import IngestContext, IngestRequest
from onyx.catalog.ingest.base.encoder import AbstractEncoder
from onyx.catalog.ingest.base.sink import Sink
from onyx.catalog.ingest.base.source import Source
from onyx.catalog.ingest.base.stream import Stream
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from onyx.shared.services.dispatcher import AbstractDispatcher


# @TODO: integrate with the read_stream_state.
# Source state still works fine but it's inefficient because required fetching from beginning if ingestion failed.
class IngestController(Logged):
    def __init__(
        self, config: OnyxConfig, dispatcher: AbstractDispatcher, encoder: AbstractEncoder, uow: AbstractUnitOfWork
    ):
        self.config = config
        self.dispatcher = dispatcher
        self.encoder = encoder
        self.storage = IntegrationStateStorage(uow=uow)

    async def process_stream(self, context: IngestContext, stream: Stream, sinks: list[Sink]):
        stream_context = stream.stream_context(context, self.config, self.encoder, self.storage)
        write_tasks = [create_task(sink.drain(stream_context)) for sink in sinks]
        count = 0
        for sink in sinks:
            await sink.create_schema(stream_context)

        async for records in stream.drip(context):
            count += len(cast(list, records))
            if not records:
                break

            for sink in sinks:
                sink.write(records)

        for sink in sinks:
            await sink.stop()

        for task in write_tasks:
            task.cancel()

        results = await gather(*write_tasks, return_exceptions=True)

        self.log.info(
            f"Finished processing stream: {stream_context.name}, {context.request_interval}: num_records={count}"
        )
        for result in results:
            if isinstance(result, CancelledError):
                continue

            if isinstance(result, Exception):
                self.log.exception(f"Error writing to sink {result}")
                raise result
        await self.storage.write_stream_state(context.identity, stream_context.name, [context.request_interval])

    async def ingest(self, source: Source, request: IngestRequest):
        interval = request.request_interval
        if not interval:
            interval = await self.storage.generate_request_interval(
                request.identity, default_beginning_delta=request.default_beginning_delta
            )
        context = IngestContext(identity=request.identity, request_interval=interval)
        try:
            async with AsyncExitStack() as exit_stack:
                streams = await exit_stack.enter_async_context(source.connect())
                stg_sink = await exit_stack.enter_async_context(
                    IbisSink.connect(context=context, config=self.config, dispatcher=self.dispatcher)
                )
                embed_sink = await exit_stack.enter_async_context(
                    VespaSink.connect(context=context, config=self.config)
                )
                await gather(*[self.process_stream(context, stream, [stg_sink, embed_sink]) for stream in streams])
            self.log.info(f"Finished ingesting source: {request}, {context.request_interval}")
            await self.storage.write_source_state(
                context.identity, last_success_bookmark=datetime.fromtimestamp(context.request_interval.end)
            )
        except Exception as exc:
            self.log.exception(f"Error ingesting source: {source}")
            await self.storage.write_source_state(context.identity, error=str(exc))
            raise exc
