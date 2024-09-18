from abc import ABC, abstractmethod
from typing import Generic, TypeVar

from aiohttp import ClientSession
from onyx.catalog.ingest.base.types import (
    APITokenConfig,
    OAuthConfig,
)
from onyx.shared.logging import Logged

C = TypeVar("C")


class Authenticator(Generic[C], ABC):
    @abstractmethod
    async def authorize(self, config: C) -> dict:
        ...


class OAuthenticator(Logged, Authenticator[OAuthConfig]):
    async def authorize(self, config) -> dict:
        self.log.info(f"Authorizing with OAuth {config}")
        async with ClientSession(
            headers=config.headers,
        ) as session:
            async with session.post(
                config.endpoint,
                json={
                    "grant_type": "refresh_token",
                    "client_id": config.client_id,
                    "client_secret": config.client_secret,
                    "refresh_token": config.refresh_token,
                },
            ) as response:
                self.log.info(await response.text())
                response.raise_for_status()
                auth_data = await response.json()
                return {
                    "Authorization": f"Bearer {auth_data['access_token']}",
                }


class APITokenAuthenticator(Authenticator[APITokenConfig]):
    async def authorize(self, config) -> dict:
        return {"Authorization": f"Bearer {config.token}"}
