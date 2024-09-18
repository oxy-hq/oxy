from abc import abstractmethod
from contextlib import asynccontextmanager
from typing import Any, ClassVar

import orjson
from aiohttp import ClientSession
from onyx.catalog.ingest.base.auth import Authenticator
from onyx.catalog.ingest.base.source import Source
from onyx.catalog.ingest.base.stream import Stream
from onyx.catalog.ingest.base.types import (
    AuthConfig,
    Response,
)


class RESTSource(Source):
    authenticator: ClassVar[Authenticator]
    base_url: ClassVar[str]

    def __init__(self, auth_config: AuthConfig) -> None:
        self.auth_config = auth_config

    @abstractmethod
    def streams(self, session: ClientSession) -> list["RESTStream"]:
        pass

    @asynccontextmanager
    async def connect(self):
        headers = await self.authenticator.authorize(self.auth_config)
        async with ClientSession(base_url=self.base_url, headers=headers) as session:
            yield self.streams(session)


class RESTStream(Stream[dict, Response]):
    def __init__(self, session: ClientSession) -> None:
        self.session = session

    async def _retrieve(self, request) -> Any:
        async with self.session.request(**request) as response:
            response.raise_for_status()
            return await response.json(loads=orjson.loads)
