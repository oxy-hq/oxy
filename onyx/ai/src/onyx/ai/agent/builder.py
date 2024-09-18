import json

from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.runnables import RunnableLambda, RunnablePassthrough
from onyx.ai.adapters.retrievers.base import CreateRetrieverFunc
from onyx.ai.adapters.tools.sql_query_tool import SQLQueryTool
from onyx.ai.adapters.warehouse_client import AbstractWarehouseClient
from onyx.ai.agent.signature import AgentNoCitationSignature, AgentSignature
from onyx.ai.agent.steps.info import AgentInfoRunnable
from onyx.ai.agent.steps.retrieval import RAGRunnable
from onyx.ai.base.builder import AbstractChainBuilder, ChainInputWithContext
from onyx.ai.base.citation import CitationMarker
from onyx.ai.base.dspy.predict import LangchainStreamPredict
from onyx.ai.base.tools import ToolsRegistry
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from onyx.shared.models.common import DataSource, TrainingPrompt
from onyx.shared.models.constants import DataSourceType


class AgentBuilder(Logged, AbstractChainBuilder):
    def __init__(
        self,
        config: OnyxConfig,
        reader_model: BaseChatModel,
        create_retriever: CreateRetrieverFunc,
        warehouse_client: AbstractWarehouseClient,
    ) -> None:
        self.reader_model = reader_model
        self.predict_module_path = config.dspy.predict_module_path
        self.create_retriever = create_retriever
        self.warehouse_client = warehouse_client

    def __build_training_instruction(self, training_prompts: list[TrainingPrompt]):
        result = ""
        for prompt in training_prompts:
            if prompt["message"] == "" or len(prompt["sources"]) == 0:
                continue
            groupnames = ",".join([source["target_embedding_table"] for source in prompt["sources"]])
            instruction = f"For queries similar to '{prompt['message']}' filter to use these groupname(s): {groupnames}"
            result += f"{instruction}\n\n"
        return result

    def __build_rag(
        self,
        data_sources: list[DataSource],
        training_prompts: list[TrainingPrompt],
        citation_marker: CitationMarker | None,
    ):
        training_instruction = self.__build_training_instruction(training_prompts)
        retriever = self.create_retriever(
            [(data_source["database"], data_source["table"]) for data_source in data_sources], training_instruction
        )
        return RAGRunnable(retriever=retriever, citation_marker=citation_marker)

    def __build_predict(self, tools: ToolsRegistry, citation_marker: CitationMarker | None = None):
        signature = AgentSignature if citation_marker else AgentNoCitationSignature
        predict = LangchainStreamPredict(
            langchain_llm=self.reader_model, signature=signature, citation_marker=citation_marker, tools=tools
        )
        try:
            with open(self.predict_module_path) as f:
                state = json.load(f)
                predict.load_state(state)
        except Exception:
            self.log.error("Failed to load state for predict", exc_info=True)

        return predict

    def __register_sql_tools(self, data_sources: list[DataSource], tools: ToolsRegistry):
        for source in data_sources:
            if source["type"] == DataSourceType.warehouse:
                self.log.info(f"Registering SQL query tool for {source['name']}")
                tool = SQLQueryTool.from_datasource(source, self.warehouse_client)
                tools.register(tool)

    def _build(
        self,
        data_sources: list[DataSource],
        training_prompts: list[TrainingPrompt],
        citation_marker: CitationMarker | None = None,
    ):
        tools = ToolsRegistry()
        warehouse_sources = [source for source in data_sources if source["type"] == DataSourceType.warehouse]
        self.__register_sql_tools(warehouse_sources, tools)

        integration_sources = [source for source in data_sources if source["type"] == DataSourceType.integration]
        rag_step = self.__build_rag(
            data_sources=integration_sources, citation_marker=citation_marker, training_prompts=training_prompts
        )
        predict_step = self.__build_predict(citation_marker=citation_marker, tools=tools)

        return (
            RunnablePassthrough[ChainInputWithContext].assign(
                chat_summary=RunnableLambda(lambda input: ""),
                relevant_information=rag_step,  # type: ignore
                agent=AgentInfoRunnable(),  # type: ignore
            )
            | predict_step
        )
