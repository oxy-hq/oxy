import asyncio

from onyx.catalog.ingest.adapters.gmail import GmailSource
from onyx.catalog.ingest.base.controller import IngestController
from onyx.catalog.ingest.base.destination import Destination
from onyx.catalog.ingest.base.storage import InMemoryStorage
from onyx.catalog.ingest.base.types import GmailOAuthConfig, Identity
from onyx.shared.config import OnyxConfig
from onyx.shared.services.dispatcher import AsyncIODispatcher


async def main():
    identity = Identity(
        namespace_id="onyx_test",
        datasource_id="onyx_datasource",
    )
    ingest = IngestController(
        storage=InMemoryStorage(),
        destination=Destination(identity=identity, config=OnyxConfig(), dispatcher=AsyncIODispatcher()),
        source=GmailSource(
            auth_config=GmailOAuthConfig.model_validate(
                {
                    "client_id": "4786469222-dlvn0v70lh8e5fffielgncmjnfq73uk3.apps.googleusercontent.com",
                    "client_secret": "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",
                    "refresh_token": "1//04RpVRnpQ1Ka6CgYIARAAGAQSNwF-L9IrRkbwVk7VWfee9uDuomU_EjyRHNtaVVqMK3md_6Rz--Yxyc-HH4szlfSrmUPHTo5mgmU",
                }
            ),
        ),
    )

    await ingest.ingest(rewrite=False)


if __name__ == "__main__":
    asyncio.run(main())
