import uuid

import pytest
import uvloop
from onyx.chat.adapters.ai_client import AbstractAIClient, FakeAIClient
from onyx.chat.adapters.catalog_client import AbstractCatalogClient, FakeCatalogClient
from onyx.chat.adapters.feedback_analytics import AbstractFeedbackAnalytics, ConsoleFeedbackAnalytics
from onyx.chat.entrypoints import chat_service
from onyx.chat.models.channel import Channel
from onyx.chat.services.unit_of_work import AbstractUnitOfWork, UnitOfWork
from onyx.shared.adapters.notify import AbstractNotification, ConsoleNotification
from onyx.shared.adapters.orm.database import create_engine, read_session_factory, sqlalchemy_session_maker
from onyx.shared.adapters.orm.mixins import sql_uow_factory
from onyx.shared.config import OnyxConfig
from onyx.shared.models.common import AgentInfo
from onyx.shared.models.handlers import DependencyRegistration
from onyx.shared.services.dispatcher import AsyncIODispatcher
from onyx.shared.services.message_bus import EventBus
from pytest_asyncio import is_async_test
from sqlalchemy import Engine
from sqlalchemy.orm import Session


@pytest.fixture(scope="session")
def event_loop_policy():
    return uvloop.EventLoopPolicy()  # type: ignore


@pytest.fixture(scope="session")
def onyx_config():
    return OnyxConfig(_env_file=".env.test")  # type: ignore


@pytest.fixture(scope="session")
def sqlalchemy_engine(onyx_config: OnyxConfig):
    return create_engine(onyx_config.database)


@pytest.fixture(scope="session")
def session_factory(sqlalchemy_engine: Engine):
    return sqlalchemy_session_maker(sqlalchemy_engine)


@pytest.fixture(scope="session")
def sqlalchemy_session(session_factory):
    with session_factory() as session:
        yield session


@pytest.fixture(scope="session")
def chat_channel(sqlalchemy_session: Session, catalog_client: FakeCatalogClient):
    agent_id = catalog_client.agent_id
    channel = Channel(name="test", organization_id=uuid.uuid4(), created_by=uuid.uuid4(), agent_id=agent_id)
    sqlalchemy_session.add(channel)
    sqlalchemy_session.commit()
    try:
        yield channel
    finally:
        sqlalchemy_session.delete(channel)
        sqlalchemy_session.commit()


@pytest.fixture(scope="session")
def ai_client():
    return FakeAIClient(
        messages=[
            "Hello! I'm a test bot.",
        ]
    )


@pytest.fixture(scope="session")
def catalog_client():
    return FakeCatalogClient(
        agent_id=uuid.uuid4(),
        agent_info=AgentInfo(
            name="test", instructions="test", description="test", knowledge="", data_sources=[], training_prompts=[]
        ),
    )


@pytest.fixture(scope="session")
def service(
    onyx_config: OnyxConfig,
    sqlalchemy_engine: Engine,
    ai_client: FakeAIClient,
    catalog_client: FakeCatalogClient,
):
    dispatcher = AsyncIODispatcher()
    event_bus = EventBus()
    chat_service.bind_event_bus(event_bus)
    chat_service.bind_dispatcher(dispatcher)
    chat_service.bind_dependencies(
        DependencyRegistration(OnyxConfig, onyx_config, is_instance=True),
        DependencyRegistration(Session, read_session_factory(sqlalchemy_engine)),
        DependencyRegistration(
            AbstractUnitOfWork,
            sql_uow_factory(session_factory=sqlalchemy_session_maker(engine=sqlalchemy_engine), cls=UnitOfWork),
        ),
        DependencyRegistration(AbstractFeedbackAnalytics, ConsoleFeedbackAnalytics),
        DependencyRegistration(AbstractNotification, ConsoleNotification),
        DependencyRegistration(AbstractAIClient, ai_client, is_instance=True),
        DependencyRegistration(AbstractCatalogClient, catalog_client, is_instance=True),
    )
    return chat_service


def pytest_collection_modifyitems(items):
    pytest_asyncio_tests = (item for item in items if is_async_test(item))
    session_scope_marker = pytest.mark.asyncio(scope="session")
    for async_test in pytest_asyncio_tests:
        async_test.add_marker(session_scope_marker)
