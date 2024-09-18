from langchain_community.utilities.google_serper import GoogleSerperAPIWrapper
from langchain_core.callbacks import CallbackManagerForRetrieverRun
from langchain_core.documents.base import Document
from langchain_core.retrievers import BaseRetriever
from onyx.ai.base.models import ReferenceSourceTypes
from onyx.shared.logging import Logged


class GoogleRetriever(Logged, BaseRetriever):
    google_serper: GoogleSerperAPIWrapper

    def _format_doc(self, title: str, link: str, snippet: str):
        return Document(
            page_content=f"{title}\n{snippet}",
            metadata={
                "source_type": ReferenceSourceTypes.web,
                "source": link,
                "url": link,
                "title": title,
            },
        )

    def _get_relevant_documents(
        self,
        query: str,
        *,
        run_manager: CallbackManagerForRetrieverRun,
    ) -> list[Document]:
        try:
            serper_result = self.google_serper.results(query)
        except Exception:
            self.log.error("Error while fetching google search results", exc_info=True)
            return []

        processed_results: list[Document] = []
        answer_box = serper_result.get("answerBox")
        if answer_box:
            link = answer_box.get("sourceLink", "")
            if link.strip() == "":
                link = f"https://www.google.com/search?q={input}"
            processed_results.append(
                self._format_doc(
                    title=answer_box.get("title", ""),
                    link=link,
                    snippet=answer_box.get("answer", ""),
                )
            )

        for o in serper_result.get("organic", []):
            processed_results.append(
                self._format_doc(
                    title=o.get("title", ""),
                    link=o.get("link", ""),
                    snippet=o.get("snippet", ""),
                )
            )
        return processed_results
