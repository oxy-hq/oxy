from langchain_core.embeddings import Embeddings
from langchain_core.language_models.chat_models import BaseChatModel
from langchain_openai import ChatOpenAI, OpenAIEmbeddings
from onyx.ai.adapters.retrievers.base import CreateRetrieverFunc
from onyx.ai.adapters.retrievers.vespa import get_vespa_retriever
from onyx.ai.adapters.tracing import AbstractTracingClient, LangfuseTracingClient
from onyx.ai.adapters.warehouse_client import AbstractWarehouseClient, WarehouseClient
from onyx.ai.agent.builder import AgentBuilder
from onyx.ai.base.builder import AbstractChainBuilder
from onyx.app import AbstractOnyxApp
from onyx.shared.config import OnyxConfig
from onyx.shared.models.handlers import DependencyRegistration
from pydantic.v1 import SecretStr


def chat_model_factory(config: OnyxConfig):
    def factory():
        return ChatOpenAI(
            api_key=SecretStr(config.openai.api_key),
            organization=config.openai.organization,
            base_url=config.openai.base_url,
            model=config.openai.chat_model,
            timeout=30,
            max_retries=5,
        )

    return factory


def embeddings_factory(config: OnyxConfig):
    def factory():
        return OpenAIEmbeddings(
            api_key=SecretStr(config.openai.api_key),
            organization=config.openai.organization,
            base_url=config.openai.base_url,
            model=config.openai.embeddings_model,
            timeout=5,
        )

    return factory


def create_retriever_wrapper(config: OnyxConfig) -> CreateRetrieverFunc:
    def create_retriever(schemas: list[tuple[str, str]], training_instruction: str = ""):
        embeddings = embeddings_factory(config)()
        llm = chat_model_factory(config)()

        return get_vespa_retriever(
            url=config.vespa.url,
            vespa_cloud_secret_token=config.vespa.cloud_secret_token or None,
            embeddings=embeddings,
            group_names=[
                schema[1]  # database is mapped to namespace, table is mapped to group name
                for schema in schemas
            ],
            llm=llm,
            training_instruction=training_instruction,
        )

    return create_retriever


def config_mapper(config: OnyxConfig, app: AbstractOnyxApp):
    return (
        DependencyRegistration(OnyxConfig, config, is_instance=True),
        DependencyRegistration(BaseChatModel, chat_model_factory(config)),
        DependencyRegistration(Embeddings, embeddings_factory(config)),
        DependencyRegistration(CreateRetrieverFunc, create_retriever_wrapper(config), is_instance=True),  # type: ignore
        DependencyRegistration(AbstractChainBuilder, AgentBuilder),
        DependencyRegistration(AbstractTracingClient, LangfuseTracingClient),
        DependencyRegistration(AbstractWarehouseClient, WarehouseClient(app.catalog), is_instance=True),
    )
