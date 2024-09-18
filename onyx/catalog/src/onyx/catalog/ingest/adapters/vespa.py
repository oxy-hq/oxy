from asyncio import gather
from contextlib import asynccontextmanager
from typing import cast

from onyx.catalog.ingest.base.context import StreamContext
from onyx.catalog.ingest.base.sink import EmbedSink
from onyx.catalog.ingest.base.types import EmbeddingRecord
from onyx.shared.logging import Logged

from vespa.application import Vespa, VespaAsync


class VespaSink(Logged, EmbedSink):
    def __init__(self, client: VespaAsync) -> None:
        self.client = client

    @classmethod
    @asynccontextmanager
    async def connect(cls, config, context):
        async with Vespa(
            url=config.vespa.url,
            vespa_cloud_secret_token=config.vespa.cloud_secret_token,
        ).asyncio() as client:
            sink = cls(client)
            yield sink

    async def create_schema(self, context):
        self.log.info("Vespa schema is predefined, skipping schema creation")

    async def __upsert(self, context: StreamContext, record: EmbeddingRecord, vespa_schema: str):
        json_record = cast(dict, record)
        record_id = json_record.pop("id")
        response = await self.client.update_data(
            schema=vespa_schema,
            data_id=record_id,
            fields=json_record,
            namespace=context.identity.embed_namespace,
            groupname=context.identity.embed_groupname,
            create=True,
            timeout=10,
        )
        self.log.info(f"Response: {response.get_json()}")
        return response

    async def _sink(self, context, records):
        total = len(records)
        self.log.info(
            f"Ingesting {total} records into {context.identity.embed_namespace} - {context.identity.embed_groupname}"
        )
        coros = []
        for record in records:
            coros.append(
                self.__upsert(
                    context=context,
                    record=record,
                    vespa_schema=context.identity.slug,
                )
            )
        await gather(*coros)
        self.log.info(f"Finished ingesting {total} records")
