import uuid
from operator import attrgetter
from typing import TypeVar

import pytest
from onyx.catalog.adapters.chat_client import AbstractChatClient, FakeChatClient
from onyx.catalog.adapters.gmail import AbstractGmail
from onyx.catalog.adapters.lock import ThreadLock
from onyx.catalog.adapters.notion import AbstractNotion
from onyx.catalog.adapters.repository import (
    AbstractAgentRepository,
    AbstractAgentVersionRepository,
    AbstractIntegrationRepository,
    AbstractNamespaceRepository,
    AbstractTaskRepository,
)
from onyx.catalog.adapters.salesforce import AbstractSalesforce
from onyx.catalog.adapters.search import AbstractSearchClient, FakeSearchClient
from onyx.catalog.adapters.slack import AbstractSlack
from onyx.catalog.adapters.task_queues import AbstractTaskQueuePublisher, TaskResult
from onyx.catalog.entrypoints.service import catalog_service
from onyx.catalog.models.agent import Agent
from onyx.catalog.models.agent_version import AgentVersion
from onyx.catalog.models.ingest_state import IngestState
from onyx.catalog.models.integration import Integration
from onyx.catalog.models.namespace import Namespace
from onyx.catalog.models.task import ExecutionType, SourceType, Task
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.adapters.orm.database import create_engine, read_session_factory, sqlalchemy_session_maker
from onyx.shared.adapters.orm.schemas import uuid_str
from onyx.shared.adapters.repository import GenericRepository
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager
from onyx.shared.adapters.worker import AbstractWorker
from onyx.shared.config import OnyxConfig
from onyx.shared.models.constants import (
    TaskQueueSystems,
)
from onyx.shared.models.handlers import DependencyRegistration
from onyx.shared.services.dispatcher import AsyncIODispatcher
from onyx.shared.services.message_bus import EventBus
from sqlalchemy import Engine
from sqlalchemy.orm import Session

T = TypeVar("T")


class InMemoryRepository(GenericRepository[T]):
    def __init__(self, key_attr: str = "id"):
        self.__key_getter: attrgetter[str] = attrgetter(key_attr)
        self.__key_setter = lambda item, value: setattr(item, key_attr, value)
        self._items: dict[str, T] = {}

    def add(self, item: T):
        if not self.__key_getter(item):
            self.__key_setter(item, uuid_str())

        item_key = self.__key_getter(item)
        self._items[str(item_key)] = item
        return item

    def get_by_id(self, key: str) -> T | None:
        return self._items.get(str(key), None)

    def get_for_update(self, id: str | uuid.UUID | int) -> T | None:
        with ThreadLock(key=id, timeout=1):
            return self.get_by_id(id)

    def list(self) -> list[T]:
        return list(self._items.values())

    def delete(self, key: str):
        del self._items[key]


class InMemoryNamespaceRepository(AbstractNamespaceRepository, InMemoryRepository[Namespace]):
    def get_default_namespace(self, organization_id: uuid.UUID) -> Namespace:
        return Namespace(
            id=organization_id,
            organization_id=organization_id,
            owner_id=organization_id,
            name="default",
        )

    def get_private_namespace(self, organization_id: uuid.UUID, user_id: uuid.UUID) -> Namespace:
        return Namespace(
            id=user_id,
            organization_id=organization_id,
            owner_id=user_id,
            name="private",
        )

    def delete_integration(self, id: str):
        pass


class InMemoryIntegrationRepository(AbstractIntegrationRepository, InMemoryRepository[Integration]):
    def get_latest_task(self, integration_id: uuid.UUID) -> Task:
        for integration in self.list():
            if integration.id == integration_id:
                return any(task for task in integration.tasks)

    def list_by_ids(self, ids: list[uuid.UUID]) -> list[Integration]:
        result = []
        for integration in self.list():
            if integration.id in ids:
                result.append(integration)
        return result

    def get_or_create_ingest_state(self, id: uuid.UUID) -> IngestState:
        integration = self.get_by_id(id)
        if not integration:
            raise ValueError(f"Integration {id} not found")
        integration.ingest_state = IngestState(integration_id=id)
        return integration.ingest_state

    def get_ingest_state_for_update(self, id: uuid.UUID) -> IngestState:
        integration = self.get_by_id(id)
        if not integration:
            raise ValueError(f"Integration {id} not found")
        if not integration.ingest_state:
            raise ValueError(f"Integration {id} has no ingest state")
        return integration.ingest_state


class InMemoryTaskRepository(AbstractTaskRepository, InMemoryRepository[Task]):
    ...


class InMemoryAgentsRepository(AbstractAgentRepository, InMemoryRepository[Agent]):
    def get_categories(self, categories):
        return []

    def list_by_subdomains(self, subdomains: list[str]) -> list[Agent]:
        return list(filter(lambda p: p.published_version.subdomain in subdomains, self.list()))

    def list_featured(self) -> list[Agent]:
        return list(filter(lambda p: p.is_featured, self.list()))


class InMemoryAgentVersionRepository(AbstractAgentVersionRepository, InMemoryRepository[AgentVersion]):
    ...


class InMemoryUnitOfWork(AbstractUnitOfWork):
    def __init__(self):
        super().__init__()
        self.integrations = InMemoryIntegrationRepository()
        self.namespaces = InMemoryNamespaceRepository()
        self.tasks = InMemoryTaskRepository()
        self.agents = InMemoryAgentsRepository()
        self.agent_versions = InMemoryAgentVersionRepository()

    def commit(self):
        pass

    def rollback(self):
        pass


class FakeTaskQueuePublisher(AbstractTaskQueuePublisher):
    system_name = TaskQueueSystems.airflow

    def publish_integration_created(self, integration: Integration) -> Task:
        return Task(
            external_id=f"fake-{integration.organization_id}-{integration.id}",
            queue_system=self.system_name,
            request_payload={"args": (integration.organization_id, integration.id)},
            source_id=integration.id,
            source_type=SourceType.integration,
            execution_type=ExecutionType.manual,
        )

    def get_task_result_by_id(self, result_id: str, slug: str) -> TaskResult:
        return TaskResult(id=result_id, state="SUCCESS", date_done=None)

    def is_task_running(self, task_id: str, slug: str) -> bool:
        return False


class FakeSecretsManager(AbstractSecretsManager):
    def encrypt(self, plaintext: str) -> str:
        return plaintext

    def decrypt(self, ciphertext: str) -> str:
        return ciphertext


@pytest.fixture(scope="session")
def in_memory_uow():
    return InMemoryUnitOfWork()


@pytest.fixture(scope="session")
def fake_task_queue():
    return FakeTaskQueuePublisher(system_name="test")


@pytest.fixture(scope="session")
def fake_secrets_manager():
    return FakeSecretsManager()


@pytest.fixture(scope="session")
def fake_salesforce():
    class FakeSalesforce(AbstractSalesforce):
        def get_refresh_token(self, code: str) -> str:
            return "token"

        def get_user_info(self, token: str) -> dict[str, str]:
            return {"email": "test@example.com"}

    return FakeSalesforce()


@pytest.fixture(scope="session")
def fake_gmail():
    class FakeGmail(AbstractGmail):
        def get_refresh_token(self, code: str) -> str:
            return "token"

        def get_user_info(self, token: str) -> dict[str, str]:
            return {"email": "test@example.com"}

    return FakeGmail()


@pytest.fixture(scope="session")
def fake_slack():
    class FakeSlack(AbstractSlack):
        def get_oauth_access(self, code: str):
            return {"token": "token", "team_id": "team_id", "team_name": "team_name"}

    return FakeSlack()


@pytest.fixture(scope="session")
def fake_notion():
    class FakeNotion(AbstractNotion):
        def get_access_token(self, code: str):
            return "token", "token"

        def get_user_info(self, token: str):
            return {}

    return FakeNotion()


@pytest.fixture(scope="session")
def dispatcher():
    return AsyncIODispatcher()


@pytest.fixture(scope="session")
def event_bus():
    return EventBus()


@pytest.fixture(scope="session")
def session_factory(sqlalchemy_engine: Engine):
    return sqlalchemy_session_maker(sqlalchemy_engine)


@pytest.fixture(scope="session")
def sqlalchemy_engine(onyx_config: OnyxConfig):
    return create_engine(onyx_config.database)


@pytest.fixture(scope="session")
def sqlalchemy_session(session_factory):
    with session_factory() as session:
        yield session


@pytest.fixture(scope="session")
def onyx_config():
    return OnyxConfig(_env_file=".env.test")  # type: ignore


@pytest.fixture(scope="session")
def fake_search_client():
    return FakeSearchClient()


@pytest.fixture(scope="session")
def worker():
    class FakeWorker(AbstractWorker):
        async def sync(self, connection_id: uuid.UUID, organization_id: uuid.UUID):
            ...

        async def ingest(self, integration_id: uuid.UUID):
            ...

    return FakeWorker()


@pytest.fixture(scope="session")
def service(
    sqlalchemy_engine: Engine,
    in_memory_uow,
    fake_secrets_manager,
    fake_task_queue,
    fake_salesforce,
    fake_gmail,
    fake_slack,
    fake_notion,
    fake_search_client,
    event_bus,
    dispatcher,
):
    catalog_service.bind_dependencies(
        DependencyRegistration(Session, read_session_factory(sqlalchemy_engine)),
        DependencyRegistration(AbstractSearchClient, fake_search_client, is_instance=True),
        DependencyRegistration(AbstractChatClient, FakeChatClient),
        DependencyRegistration(AbstractUnitOfWork, in_memory_uow, is_instance=True),
        DependencyRegistration(AbstractSecretsManager, fake_secrets_manager, is_instance=True),
        DependencyRegistration(AbstractTaskQueuePublisher, fake_task_queue, is_instance=True),
        DependencyRegistration(AbstractSalesforce, fake_salesforce, is_instance=True),
        DependencyRegistration(AbstractGmail, fake_gmail, is_instance=True),
        DependencyRegistration(AbstractSlack, fake_slack, is_instance=True),
        DependencyRegistration(AbstractNotion, fake_notion, is_instance=True),
    )
    catalog_service.bind_event_bus(event_bus)
    catalog_service.bind_dispatcher(dispatcher)
    return catalog_service
