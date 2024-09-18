from onyx.catalog.ingest.base.encoder import AbstractEncoder
from onyx.shared.config import OnyxConfig
from openai import AsyncOpenAI


class OpenAIEncoder(AbstractEncoder):
    def __init__(self, config: OnyxConfig) -> None:
        self.client = AsyncOpenAI(
            api_key=config.openai.api_key,
        )
        self.model = config.openai.embeddings_model

    async def encode(self, chunks: list[str]) -> dict[str, list[float]]:
        embeddings_response = await self.client.embeddings.create(
            input=chunks,
            model=self.model,
            timeout=5,
        )
        embeddings = embeddings_response.data
        results = {str(idx): embedding.embedding for idx, embedding in enumerate(embeddings)}
        return results
