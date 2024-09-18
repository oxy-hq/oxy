import json
from datetime import datetime
from typing import Any, ClassVar, Collection, Dict

from langchain.chains.query_constructor.base import AttributeInfo
from langchain.retrievers.self_query.base import SelfQueryRetriever
from langchain_community.vectorstores.vespa import VespaStore as LangchainVespaStore
from langchain_core.callbacks.manager import (
    CallbackManagerForRetrieverRun,
)
from langchain_core.documents.base import Document
from langchain_core.embeddings import Embeddings
from langchain_core.language_models import BaseChatModel
from langchain_core.prompts import PromptTemplate
from langchain_core.vectorstores import VectorStoreRetriever
from onyx.ai.adapters.retrievers.filtering_metadata_schema import (
    filtering_metadata_schema,
)
from onyx.ai.adapters.retrievers.vespa_translator import VespaTranslator
from onyx.shared.logging import Logged

from vespa.application import Vespa

CUSTOM_QUERY_CONSTRUCTOR_SCHEMA = """\
    << Structured Request Schema >>
    When responding use a markdown code snippet with a JSON object formatted in the following schema:

    ```json
    {{{{
        "query": string \\ text string to compare to document contents
        "filter": string \\ logical condition statement for filtering documents
    }}}}
    ```
    Today is {date}

    The query string should contain only text that is expected to match the contents of documents. Any conditions in the filter should not be mentioned in the query as well.

    A logical condition statement is composed of one or more comparison and logical operation statements.

    A comparison statement takes the form: `comp(attr, val)`:
    - `comp` ({allowed_comparators}): comparator
    - `attr` (string):  name of attribute to apply the comparison to
    - `val` (string): is the comparison value

    A logical operation statement takes the form `op(statement1, statement2, ...)`:
    - `op` ({allowed_operators}): logical operator
    - `statement1`, `statement2`, ... (comparison statements or logical operation statements): one or more statements to apply the operation to


    Instructions:
    {training_instruction}

    Make sure that you only use the comparators and logical operators listed above and no others.
    Make sure that filters only refer to attributes that exist in the data source.
    Make sure that filters only use the attributed names with its function names if there are functions applied on them.
    Make sure that filters only use format `YYYY-MM-DD` when handling date data typed values.
    Make sure that filters take into account the descriptions of attributes and only make comparisons that are feasible given the type of data being stored.
    Make sure that filters are only used as needed. If there are no filters that should be applied return "NO_FILTER" for the filter value.\
    """


class VespaRetriever(Logged, VectorStoreRetriever, SelfQueryRetriever):
    allowed_search_types: ClassVar[Collection[str]] = (
        "similarity",
        "similarity_score_threshold",
        "mmr",
        "hybrid",
    )
    search_type: str = "hybrid"

    group_names: list[str]
    top_k: int

    async def ___prepare_filter_search_kwargs(self, query: str, run_manager: "CallbackManagerForRetrieverRun"):
        try:
            structured_query = await self.query_constructor.ainvoke(
                {"query": query}, config={"callbacks": run_manager.get_child()}
            )
            _, search_kwargs = self._prepare_query(query, structured_query)

            filter = search_kwargs["filter"] if "filter" in search_kwargs else None
            top_k = search_kwargs["k"] if "k" in search_kwargs else self.top_k
            return {"filter": filter, "k": top_k}
        except Exception:
            self.log.error("Error while preparing filter search kwargs", exc_info=True)
            return {}

    async def _aget_relevant_documents(
        self, query: str, *, run_manager: "CallbackManagerForRetrieverRun"
    ) -> "list[Document]":
        filter_kwargs = await self.___prepare_filter_search_kwargs(query, run_manager)
        search_kwargs = {
            "group_names": self.group_names,
            "target_hits": 1000,
            **filter_kwargs,
        }

        if self.search_type in ("hybrid", "similarity"):
            docs = await self.vectorstore.asearch(query, search_type=self.search_type, **search_kwargs)
        elif self.search_type == "similarity_score_threshold":
            docs_and_similarities = self.vectorstore.similarity_search_with_relevance_scores(query, **search_kwargs)
            docs = [doc for doc, _ in docs_and_similarities]
        else:
            raise ValueError(f"search_type of {self.search_type} not allowed.")
        return docs


class VespaStore(Logged, LangchainVespaStore):
    @property
    def embeddings(self):
        return self._embedding_function

    async def __get_query(self, query: str, k: int = 4, **kwargs) -> Dict:
        query_embedding: list[float] = await self.embeddings.aembed_query(query)
        hits = k
        doc_embedding_field = self._embedding_field
        input_embedding_field = self._input_field
        filter = kwargs["filter"] if "filter" in kwargs else None
        target_hits = kwargs["target_hits"] if "target_hits" in kwargs else 1000
        search_type = kwargs["search_type"] if "search_type" in kwargs else "hybrid"

        ranking_function = "semantic"
        if search_type == "hybrid":
            ranking_function = "hybrid"

        nearest_neighbor_expression = (
            f"{{targetHits:{target_hits}}}nearestNeighbor({doc_embedding_field},{input_embedding_field})"
        )
        yql = "select * from sources * where "

        if search_type == "hybrid":
            if filter is not None:
                yql += f"rank(userQuery(), {nearest_neighbor_expression}, {filter})"
            else:
                yql += f"rank(userQuery(), {nearest_neighbor_expression})"
        else:
            yql += f"{nearest_neighbor_expression}"
            if filter is not None:
                yql += f" and {filter}"

        vespa_query = {
            "yql": yql,
            "input.query(q)": query_embedding,
            "ranking": ranking_function,
            "hits": hits,
        }
        if search_type == "hybrid":
            vespa_query["query"] = query

        return vespa_query

    def __process_metadata(self, fields: dict, metadata: dict) -> dict:
        metadata_fields = self._metadata_fields or []
        for field in metadata_fields:
            metadata[field] = fields.get(field)

        if "metadata" in metadata:
            for meta_field in metadata["metadata"]:
                key, value = meta_field.split("===")
                metadata[key] = value
        return metadata

    def __get_page_content(self, hit):
        fields: dict = hit["fields"]
        closest_chunk_ids: list[str] = list(
            fields.get("matchfeatures", {}).get("closest(embeddings)", {}).get("cells", {}).keys()
        )
        chunks: list[str] = []

        if not closest_chunk_ids:
            chunks = fields.get(self._page_content_field, [])
        else:
            idx = int(closest_chunk_ids[0])
            start = max(0, idx - 1)
            end = idx + 1
            chunks.extend(fields[self._page_content_field][start:end])

        page_content = "\n".join(chunks)
        return page_content

    async def asimilarity_search_with_score(
        self, query: str, k: int = 4, **kwargs: Any
    ) -> list[tuple[Document, float]]:
        return await self.__vespa_search(query, k, **kwargs)

    async def asearch(self, query: str, search_type: str, **kwargs: Any) -> list[Document]:
        """Return docs most similar to query using specified search type.

        Args:
            query: Input text.
            search_type: Type of search to perform. Can be "similarity",
                "mmr", or "similarity_score_threshold".
            **kwargs: Arguments to pass to the search method.
        """
        if search_type in ("similarity", "hybrid", "similarity_score_threshold"):
            docs_and_similarities = await self.asimilarity_search_with_score(query, **kwargs)
            return [doc for doc, _ in docs_and_similarities]
        elif search_type == "mmr":
            return await self.amax_marginal_relevance_search(query, **kwargs)
        else:
            raise ValueError(
                f"search_type of {search_type} not allowed. Expected "
                "search_type to be 'similarity', 'similarity_score_threshold' or 'mmr'."
            )

    def __parse_timestamp(self, timestamp: str) -> datetime | None:
        try:
            return datetime.fromtimestamp(float(timestamp))
        except TypeError:
            return None

    async def __vespa_search(self, query: str, k: int = 4, **kwargs) -> list[tuple[Document, float]]:
        vespa_query = await self.__get_query(query, k, **kwargs)
        group_names = kwargs.get("group_names", [])
        if not group_names:
            return []

        selection = " or ".join(f'id.group == "{group_name}"' for group_name in group_names).lstrip().rstrip()
        group_kwargs = {}
        if selection:
            group_kwargs["streaming.selection"] = selection

        try:
            async with self._vespa_app.asyncio() as client:
                response = await client.query(body=vespa_query, timeout="30s", **group_kwargs)
        except Exception as e:
            raise RuntimeError("Could not retrieve data from Vespa") from e

        root = response.json["root"]
        if "errors" in root:
            raise RuntimeError(json.dumps(root["errors"]))

        if response is None or response.hits is None:
            return []

        self.log.debug("Vespa response", response=response.json)
        docs = []
        for child in response.hits:
            fields = child["fields"]
            score = 1.0 if child["relevance"] == "NaN" else child["relevance"]
            metadata = {"id": child["id"]}
            page_content = self.__get_page_content(child)
            if self._metadata_fields is not None:
                metadata = self.__process_metadata(fields, metadata)

                if "timestamp" in metadata:
                    metadata["timestamp"] = self.__parse_timestamp(metadata["timestamp"])

            doc = Document(page_content=page_content, metadata=metadata)
            docs.append((doc, score))
        self.log.debug("Vespa retriever document", docs=docs)
        return docs


def get_vespa_vector_store(
    embeddings: Embeddings,
    url: str,
    vespa_cloud_secret_token: str | None = None,
    vespa_store_cls: type["VespaStore"] = VespaStore,
):
    vespa_config = {
        "page_content_field": "chunks",
        "embedding_field": "embeddings",
        "input_field": "q",
        "metadata_fields": ["timestamp", "metadata", "title"],
    }
    vespa_app = Vespa(
        url=url,
        vespa_cloud_secret_token=vespa_cloud_secret_token,
    )
    vespa_store = vespa_store_cls(app=vespa_app, embedding_function=embeddings, **vespa_config)
    return vespa_store


def get_vespa_retriever(
    embeddings: Embeddings,
    url: str,
    group_names: list[str],
    vespa_cloud_secret_token: str | None = None,
    top_k: int = 4,
    training_instruction: str = "",
    llm: BaseChatModel = None,
):
    vespa_store = get_vespa_vector_store(embeddings, url, vespa_cloud_secret_token)

    now = datetime.now()

    if not training_instruction:
        training_instruction = "None"
    CUSTOM_QUERY_CONSTRUCTOR_SCHEMA_PROMPT = PromptTemplate.from_template(CUSTOM_QUERY_CONSTRUCTOR_SCHEMA)

    now_str = now.strftime("%Y-%m-%d")
    query_constructor_schema_prompt = CUSTOM_QUERY_CONSTRUCTOR_SCHEMA_PROMPT.partial(
        date=now_str, training_instruction=training_instruction
    )

    filtering_metadata_fields = filtering_metadata_schema.fields
    group_name_options = ", ".join(group_names)
    # add groupname into metadata fields for filtering
    filtering_metadata_fields.append(
        AttributeInfo(
            name="groupname",
            description=f"The group names of documents. Options: {group_name_options}",
            type="list of string separated by comma",
        )
    )

    return VespaRetriever.from_llm(
        vectorstore=vespa_store,
        group_names=group_names,
        top_k=top_k,
        llm=llm,
        document_contents=filtering_metadata_schema.content_description,
        metadata_field_info=filtering_metadata_schema.fields,
        structured_query_translator=VespaTranslator(),
        chain_kwargs={
            "schema_prompt": query_constructor_schema_prompt,
        },
    )
