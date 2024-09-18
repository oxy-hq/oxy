import json
from datetime import datetime

from dsp.modules.sentence_vectorizer import BaseSentenceVectorizer
from dsp.utils import dotdict
from dspy.retrieve.retrieve import Retrieve

from vespa.application import Vespa
from vespa.io import VespaQueryResponse

default_metadata_fields = ["timestamp", "metadata", "title"]


class VespaQueryBuilder:
    def __init__(
        self,
        embedding_field: str = "embeddings",
        input_field: str = "q",
    ):
        self.__embedding_field = embedding_field
        self.__input_field = input_field

    def build(self, query: str, embeddings: list[float], k: int = 4, **kwargs) -> dict:
        doc_embedding_field = self.__embedding_field
        input_embedding_field = self.__input_field
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
            "input.query(q)": embeddings,
            "ranking": ranking_function,
            "hits": k,
        }
        if search_type == "hybrid":
            vespa_query["query"] = query

        return vespa_query


class VespaResponseParser:
    def __init__(
        self,
        paragraph_expansion: int = 1,
        page_content_field: str = "chunks",
        metadata_fields: list[str] = default_metadata_fields,
    ) -> None:
        self.__paragraph_expansion = paragraph_expansion
        self.__metadata_fields = metadata_fields
        self.__page_content_field = page_content_field

    def __get_page_content(self, hit):
        fields: dict = hit["fields"]
        closest_chunk_ids: list[str] = list(
            fields.get("matchfeatures", {}).get("closest(embeddings)", {}).get("cells", {}).keys()
        )
        chunks: list[str] = []

        if not closest_chunk_ids:
            chunks = fields.get(self.__page_content_field, [])
        else:
            idx = int(closest_chunk_ids[0])
            start = max(0, idx - self.__paragraph_expansion)
            end = idx + self.__paragraph_expansion
            chunks.extend(fields[self.__page_content_field][start:end])

        page_content = "\n".join(chunks)
        return page_content

    def __process_metadata(self, fields: dict, metadata: dict) -> dict:
        metadata_fields = self.__metadata_fields or []
        for field in metadata_fields:
            metadata[field] = fields.get(field)

        if "metadata" in metadata:
            for meta_field in metadata["metadata"]:
                key, value = meta_field.split("===")
                metadata[key] = value
        return metadata

    def parse(self, response: VespaQueryResponse) -> list[dotdict]:
        root = response.json["root"]
        if "errors" in root:
            raise RuntimeError(json.dumps(root["errors"]))

        if response is None or response.hits is None:
            return []

        docs: list[dotdict] = []

        for child in response.hits:
            fields = child["fields"]
            score = 1.0 if child["relevance"] == "NaN" else child["relevance"]
            metadata = {"id": child["id"]}
            page_content = self.__get_page_content(child)
            if self.__metadata_fields is not None:
                metadata = self.__process_metadata(fields, metadata)

                if "timestamp" in metadata:
                    metadata["timestamp"] = datetime.fromtimestamp(float(metadata["timestamp"])).isoformat()

            doc = dotdict(long_text=page_content, metadata=metadata, score=score)
            docs.append(doc)
        return docs


class VespaRM(Retrieve):
    def __init__(
        self,
        vespa_client: Vespa,
        vectorizer: BaseSentenceVectorizer,
        timeout: str = "30s",
        paragraph_expansion: int = 1,
        embedding_field: str = "embeddings",
        input_field: str = "q",
        page_content_field: str = "chunks",
        metadata_fields: list[str] = default_metadata_fields,
        k=4,
    ):
        self.__client = vespa_client
        self.__vectorizer = vectorizer
        self.__timeout = timeout
        self.__response_parser = VespaResponseParser(
            paragraph_expansion=paragraph_expansion,
            page_content_field=page_content_field,
            metadata_fields=metadata_fields,
        )
        self.__query_builder = VespaQueryBuilder(embedding_field=embedding_field, input_field=input_field)

        super().__init__(k)

    def forward(self, query_or_queries: str | list[str], k: int | None = None, **kwargs):
        group_names = kwargs.get("group_names", None)
        if not group_names:
            return []

        selection = " or ".join(f'id.group == "{group_name}"' for group_name in group_names).lstrip().rstrip()
        group_kwargs = {}
        if selection:
            group_kwargs["streaming.selection"] = selection

        k = k if k is not None else self.k
        queries = [query_or_queries] if isinstance(query_or_queries, str) else query_or_queries
        queries = [q for q in queries if q]
        vectors = self.__vectorizer(queries).tolist()
        passages = []
        for query, embeddings in zip(queries, vectors):
            vespa_query = self.__query_builder.build(query, embeddings, k, **kwargs)
            response = self.__client.query(body=vespa_query, timeout=self.__timeout, **group_kwargs)
            parsed_results = self.__response_parser.parse(response)
            passages.extend(parsed_results)

        return passages
