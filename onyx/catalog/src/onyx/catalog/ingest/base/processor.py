import abc
from typing import (
    Generic,
    TypeVar,
)

from onyx.catalog.ingest.base.encoder import AbstractEncoder
from onyx.catalog.ingest.base.types import EmbeddingRecord, Record
from onyx.shared.config import OnyxConfig
from semantic_text_splitter import TextSplitter

T = TypeVar("T", bound=Record)


class ProcessingStrategy(Generic[T], abc.ABC):
    def __init__(
        self,
        config: OnyxConfig,
        encoder: AbstractEncoder,
        stream_name: str,
    ):
        self.encoder = encoder
        self.stream_name = stream_name
        self.tiktoken_model = config.openai.chat_model
        self.capacity = config.openai.embeddings_max_tokens

    @property
    def text_splitter(self):
        return TextSplitter.from_tiktoken_model(self.tiktoken_model, capacity=self.capacity)

    async def process_record(self, record: T) -> EmbeddingRecord:
        document = self._build_doc(record)
        chunks = self.text_splitter.chunks(document)
        embeddings = await self.encoder.encode(chunks)
        return self._conform_record(record, chunks, embeddings)

    def _conform_record(self, record: T, chunks: list[str], embeddings: dict[str, list[float]]) -> EmbeddingRecord:
        return EmbeddingRecord(
            id=self._build_doc_id(record),
            title=self._build_doc_title(record),
            chunks=chunks,
            embeddings=embeddings,
            metadata=self._build_metadata(record),
            timestamp=self._build_timestamp(record),
        )

    def _build_metadata(self, record: T) -> list[str]:
        return [
            f"source_type==={self.stream_name}",
            f"source==={self._build_doc_id(record)}",
            f"url==={self._build_doc_url(record)}",
        ]

    @abc.abstractmethod
    def _build_doc_id(self, record: T) -> str:
        ...

    @abc.abstractmethod
    def _build_timestamp(self, record: T) -> int:
        ...

    @abc.abstractmethod
    def _build_doc_url(self, record: T) -> str:
        ...

    @abc.abstractmethod
    def _build_doc_title(self, record: T) -> str:
        ...

    @abc.abstractmethod
    def _build_doc(self, record: T) -> str:
        ...
