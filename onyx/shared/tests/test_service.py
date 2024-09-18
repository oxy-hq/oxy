import asyncio
from abc import ABC, abstractmethod
from contextlib import asynccontextmanager, contextmanager
from typing import AsyncIterable, Protocol, Self

import pytest
from onyx.shared.models.base import Command, Event, Message
from onyx.shared.models.handlers import DependencyRegistration
from onyx.shared.services.base import Service
from onyx.shared.services.dispatcher import AsyncIODispatcher
from onyx.shared.services.message_bus import EventBus, EventCollector
from onyx.shared.services.unit_of_work import AbstractUnitOfWork


class CountEvent(Protocol):
    count: int


class AbstractAdapter(ABC):
    @abstractmethod
    def invoke(self, text: str) -> str:
        pass

    @abstractmethod
    def increase(self, event: "CountEvent"):
        pass


class EchoAdapter(AbstractAdapter):
    def invoke(self, text: str):
        return text

    def increase(self, event: "CountEvent"):
        event.count += 1


class PingQuery(Message[str]):
    text: str


class PingCommand(Command[int]):
    count: int


class PongEvent(Event):
    request: PingCommand


def dummy_handler(request: PingQuery, adapter: AbstractAdapter):
    return adapter.invoke(request.text)


def dummy_publish_event_handler(request: PingCommand, collector: EventCollector):
    collector.publish(PongEvent(request=request))
    return request.count


async def event_handler(event: PongEvent, adapter: AbstractAdapter):
    await asyncio.sleep(0.01)
    adapter.increase(event.request)


class FakeUnitOfWork(AbstractUnitOfWork):
    def commit(self):
        pass

    def rollback(self):
        pass


class DecoratedCommand(Command[str]):
    text: str
    count: int


class DecoratedEvent(Event):
    request: DecoratedCommand


service = Service("dummy-service")


@service.register_request
def decorated_command_handler(request: DecoratedCommand, adapter: AbstractAdapter, event_collector: EventCollector):
    event_collector.publish(DecoratedEvent(request=request))
    return adapter.invoke("hello")


@service.register_event
def decorated_event_handler(event: DecoratedEvent, adapter: AbstractAdapter):
    adapter.increase(event.request)


class AbstractContextAdapter(ABC):
    is_open: bool

    def __enter__(self):
        return self.context()

    def __exit__(self, *args):
        self.close()

    @abstractmethod
    def context(self) -> Self:
        pass

    @abstractmethod
    def close(self):
        pass


class AbstractAsyncContextManager(ABC):
    is_open: bool

    async def __aenter__(self):
        return await self.acontext()

    async def __aexit__(self, *args):
        return await self.aclose()

    @abstractmethod
    async def acontext(self) -> Self:
        pass

    @abstractmethod
    async def aclose(self):
        pass


class FakeContextAdapter(AbstractContextAdapter):
    def __init__(self):
        self.is_open = False

    def context(self):
        self.is_open = True
        return self

    def close(self):
        self.is_open = False


class FakeAsyncContextAdapter(AbstractAsyncContextManager):
    def __init__(self):
        self.is_open = False

    async def acontext(self):
        self.is_open = True
        return self

    async def aclose(self):
        self.is_open = False


@contextmanager
def context_factory():
    with FakeContextAdapter() as ctx:
        yield ctx


@asynccontextmanager
async def async_context_factory():
    async with FakeAsyncContextAdapter() as ctx:
        yield ctx


class ContextRequest(Message[tuple[bool, bool]]):
    is_async_context_open: bool = False


class ContextEvent(Event):
    request: ContextRequest


@service.register_request
def context_handlder(request: ContextRequest, collector: EventCollector, adapter: AbstractContextAdapter):
    with adapter as ctx:
        in_context_state = ctx.is_open
    after_context_state = adapter.is_open
    collector.publish(ContextEvent(request=request))
    return in_context_state, after_context_state


@service.register_event
async def async_context_handler(event: ContextEvent, adapter: AbstractAsyncContextManager):
    event.request.is_async_context_open = adapter.is_open


class StreamingRequest(Message[AsyncIterable[str]]):
    ...


@service.register_request
async def streaming_handler(event: StreamingRequest, adapter: AbstractContextAdapter):
    for _ in range(10):
        yield adapter.is_open


uow = FakeUnitOfWork()
dispatcher = AsyncIODispatcher()
service.with_handlers(dummy_handler, dummy_publish_event_handler, event_handler)
service.bind_dependencies(
    DependencyRegistration(AbstractContextAdapter, context_factory),
    DependencyRegistration(AbstractAsyncContextManager, FakeAsyncContextAdapter),
    DependencyRegistration(AbstractAdapter, EchoAdapter),
    DependencyRegistration(AbstractUnitOfWork, uow, is_instance=True),
)
service.bind_event_bus(EventBus())
service.bind_dispatcher(dispatcher)


@pytest.mark.asyncio(scope="session")
async def test_service_inject_context():
    response = await service.handle(PingQuery(text="hello"))
    assert response == "hello"


@pytest.mark.asyncio(scope="session")
async def test_service_with_async_event_handler():
    request = PingCommand(count=0)
    count = await service.handle(request)
    await asyncio.sleep(0.1)
    assert count == 0
    assert request.count == 1


@pytest.mark.asyncio(scope="session")
async def test_service_with_decorated_handler():
    request = DecoratedCommand(text="hello", count=0)
    response = await decorated_command_handler(request)
    await asyncio.sleep(0.01)
    assert response == "hello"
    assert request.count == 1


@pytest.mark.asyncio(scope="session")
async def test_service_with_context_manager_deps():
    request = ContextRequest()
    in_context_state, after_context_state = await service.handle(request)
    assert in_context_state is True
    assert after_context_state is False
    await asyncio.sleep(0.01)
    assert request.is_async_context_open is True


@pytest.mark.asyncio(scope="session")
async def test_service_handle_async_generator():
    request = StreamingRequest()
    async for is_open in service.handle(request):
        assert is_open
