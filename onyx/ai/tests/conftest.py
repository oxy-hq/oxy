import pytest
import uvloop
from langchain_core.embeddings import Embeddings, FakeEmbeddings
from langchain_core.language_models import BaseChatModel, FakeListChatModel
from langchain_core.retrievers import BaseRetriever
from onyx.ai.adapters.retrievers.base import CreateRetrieverFunc
from onyx.ai.adapters.tracing import AbstractTracingClient, NoopTracingClient
from onyx.ai.adapters.warehouse_client import AbstractWarehouseClient, FakeWarehouseClient
from onyx.ai.agent.builder import AgentBuilder
from onyx.ai.base.builder import AbstractChainBuilder
from onyx.ai.entrypoints import ai_service
from onyx.shared.config import OnyxConfig
from onyx.shared.models.handlers import DependencyRegistration
from onyx.shared.services.dispatcher import AsyncIODispatcher
from onyx.shared.services.message_bus import EventBus
from pytest_asyncio import is_async_test


@pytest.fixture(scope="session")
def event_loop_policy():
    return uvloop.EventLoopPolicy()  # type: ignore


def pytest_collection_modifyitems(items):
    pytest_asyncio_tests = (item for item in items if is_async_test(item))
    session_scope_marker = pytest.mark.asyncio(scope="session")
    for async_test in pytest_asyncio_tests:
        async_test.add_marker(session_scope_marker)


@pytest.fixture(scope="session")
def onyx_config():
    return OnyxConfig(_env_file=".env.test")  # type: ignore


@pytest.fixture(scope="session")
def fake_chat_model():
    return FakeListChatModel(
        responses=[
            "I'm a fake chat model",
        ]
    )


@pytest.fixture(scope="session")
def fake_embeddings():
    return FakeEmbeddings(size=1024)


@pytest.fixture(scope="session")
def fake_retriever():
    class FakeRetriever(BaseRetriever):
        def _get_relevant_documents(self, query: str, *, run_manager):
            return []

    return FakeRetriever()


@pytest.fixture(scope="session")
def service(
    onyx_config: OnyxConfig,
    fake_retriever,
    fake_chat_model,
    fake_embeddings,
):
    dispatcher = AsyncIODispatcher()
    event_bus = EventBus()

    def create_retriever(schemas: list[tuple[str, str]], training_instruction: str):
        return fake_retriever

    ai_service.bind_event_bus(event_bus)
    ai_service.bind_dispatcher(dispatcher)
    ai_service.bind_dependencies(
        DependencyRegistration(OnyxConfig, onyx_config, is_instance=True),
        DependencyRegistration(BaseChatModel, fake_chat_model, is_instance=True),
        DependencyRegistration(Embeddings, fake_embeddings, is_instance=True),
        DependencyRegistration(CreateRetrieverFunc, create_retriever, is_instance=True),  # type: ignore
        DependencyRegistration(AbstractChainBuilder, AgentBuilder),
        DependencyRegistration(AbstractTracingClient, NoopTracingClient),
        DependencyRegistration(AbstractWarehouseClient, FakeWarehouseClient),
    )
    return ai_service
