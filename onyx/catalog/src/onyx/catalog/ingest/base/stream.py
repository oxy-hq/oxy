from abc import ABC, abstractmethod
from typing import Any, Generic, Iterable

from onyx.catalog.ingest.base.context import IngestContext, StreamContext
from onyx.catalog.ingest.base.encoder import AbstractEncoder
from onyx.catalog.ingest.base.storage import Storage
from onyx.catalog.ingest.base.types import Record, Request, Response
from onyx.shared.config import OnyxConfig


class Stream(Generic[Request, Response], ABC):
    @abstractmethod
    async def _retrieve(self, request: Request) -> Response:
        pass

    @abstractmethod
    def _request_factory(self, context: IngestContext) -> Request:
        pass

    @abstractmethod
    def _extract_cursor(self, response: Response) -> Any | None:
        pass

    @abstractmethod
    def _merge_cursor(self, request: Request, cursor: Any | None) -> Request:
        pass

    @abstractmethod
    async def _extract_records(self, response: Response) -> Iterable[Record]:
        pass

    @abstractmethod
    def stream_context(
        self, context: IngestContext, config: OnyxConfig, encoder: AbstractEncoder, storage: Storage
    ) -> StreamContext:
        pass

    async def drip(self, context: IngestContext):
        request = self._request_factory(context)
        cursor = None
        while True:
            new_request = self._merge_cursor(request, cursor)
            response = await self._retrieve(new_request)
            records = await self._extract_records(response)
            yield records

            cursor = self._extract_cursor(response)
            if not cursor:
                break
