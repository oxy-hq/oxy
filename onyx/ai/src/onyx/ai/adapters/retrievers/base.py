from typing import Callable

from langchain_core.retrievers import BaseRetriever

CreateRetrieverFunc = Callable[[list[tuple[str, str]]], BaseRetriever]
