from langchain_core.documents.base import Document
from langchain_core.retrievers import BaseRetriever
from langchain_core.runnables import RunnableSerializable
from langchain_core.runnables.config import RunnableConfig
from onyx.ai.base.builder import ChainInput
from onyx.ai.base.citation import CitationMarker
from onyx.shared.logging import Logged
from onyx.shared.models.common import Step, StreamingChunk


class RAGRunnable(Logged, RunnableSerializable[ChainInput, str]):
    retriever: BaseRetriever
    empty_response: str = "<empty>"
    separator: str = "\n___\n"
    block_template: str = "```\n{}\n```"
    citation_marker: CitationMarker | None = None

    class Config:
        arbitrary_types_allowed = True

    def _format_doc(self, doc: Document, config: RunnableConfig | None = None):
        content = self.block_template.format(doc.page_content)
        metadata_str = "\n".join(f"{k.upper()}: {v}" for k, v in doc.metadata.items())
        if self.citation_marker:
            return f"""{self.citation_marker.get_citation(doc)} :
{metadata_str}
{content}
"""

        return f"""{metadata_str}
{content}
"""

    def _format_context(self, docs: list[Document], config: RunnableConfig | None = None):
        self.log.info(f"Formatting documents {docs}")
        documents_str = self.separator.join(self._format_doc(doc, config) for doc in docs)
        return f"""DOCUMENTS:
---
{documents_str}
---
"""

    async def _run(self, queries: list[str], config: RunnableConfig | None = None) -> str:
        """Use the tool."""
        all_documents = []
        for query in queries:
            documents = await self.retriever.ainvoke(query, config)
            all_documents.extend(documents)

        return self._format_context(all_documents, config)

    async def ainvoke(self, input, config, **kwargs):
        return await self._run([input["message"]], config)

    def invoke(self, input, config, **kwargs):
        all_documents = []
        for query in [input["message"]]:
            documents = self.retriever.invoke(query, config)
            all_documents.extend(documents)

        return self._format_context(all_documents, config)

    async def astream(
        self,
        input,
        config=None,
        **kwargs,
    ):
        yield StreamingChunk.step(Step.FetchData)
        async for chunk in super().astream(input, config, **kwargs):
            yield chunk
