import inspect
from contextlib import AsyncExitStack, aclosing, contextmanager
from typing import (
    AsyncContextManager,
    AsyncGenerator,
    AsyncIterable,
    Awaitable,
    Callable,
    ContextManager,
    cast,
)

from onyx.shared.logging import Logged
from onyx.shared.models.base import (
    Event,
    Message,
    ResponseType,
)
from onyx.shared.models.handlers import (
    DependencyRegistration,
    EventBusHandler,
    EventHandler,
    InjectedRequestHandler,
    P,
    RequestHandler,
    RequestType,
    T,
)
from onyx.shared.services.di import DependenciesResolver
from onyx.shared.services.dispatcher import AbstractDispatcher
from onyx.shared.services.message_bus import EventBus, EventCollector, EventType


class Service(Logged):
    """
    Base class for services in the application.

    Attributes:
        name (str): The name of the service.
        __handlers (dict[type, RequestHandler]): A dictionary mapping request types to request handlers.
        __subscriptions (list[tuple[type, EventHandler]]): A list of event subscriptions.

    Methods:
        with_request_handlers: Adds request handlers to the service.
        with_event_handlers: Adds event handlers to the service.
        register_request: Registers a request handler.
        register_event: Registers an event handler.
        bind_dependencies: Binds dependencies to the service.
        bind_event_bus: Binds an event bus to the service.
        handle: Handles a message by invoking the appropriate handler.

    Example:
    ```
    event_bus = EventBus()

    class MyMessage(Message):
        text: str

    class MyEvent(Event):
        pass

    def request_handler(request: MyMessage, dependency: MyDependency, publish: PublishHandler):
        # Handle request
        # Publish events
        dependency.do_something()
        publish(MyEvent())
        return request.text

    def event_handler(event: MyEvent, dependency: MyDependency):
        # Handle event
        dependency.do_something()

    my_service = Service("my-service")
        .with_request_handlers(request_handler)
        .with_event_handlers(event_handler)
        .bind_dependencies(
            (AbstractOrDependency, MyDependency()),
        )
        .bind_event_bus(event_bus)

    # request
    request = Request[MyMessage, str](message=MyMessage(text="Hello"))
    response = my_service.handle(request)
    ```
    """

    dependencies_resolver: DependenciesResolver
    event_bus: EventBus | None

    def __init__(self, name: str) -> None:
        self.name = name
        self.event_bus = None
        self.dependencies_resolver = DependenciesResolver()
        self.__dispatcher: AbstractDispatcher | None = None
        self.__handlers: "dict[type, InjectedRequestHandler]" = {}
        self.__subscriptions: "list[tuple[type, EventBusHandler]]" = []

    @property
    def dispatcher(self) -> AbstractDispatcher:
        if not self.__dispatcher:
            raise ValueError("Dispatcher not bound to service")
        return self.__dispatcher

    def with_handlers(self, *handlers: "EventHandler | RequestHandler"):
        for handler in handlers:
            self.__register_handler(handler)
        return self

    def register_request(self, handler: "RequestHandler[RequestType, P, ResponseType]"):
        return cast(InjectedRequestHandler[RequestType, ResponseType], self.__register_handler(handler))

    def register_event(self, handler: "EventHandler[EventType, P]"):
        return cast(EventBusHandler[EventType], self.__register_handler(handler))

    def bind_dependencies(
        self,
        *dependencies: "DependencyRegistration",
    ):
        self.dependencies_resolver.register(*dependencies)
        return self

    def bind_event_bus(self, event_bus: EventBus):
        self.event_bus = event_bus
        self.__delegate_subscriptions(event_bus)
        return self

    def bind_dispatcher(self, dispatcher: AbstractDispatcher):
        self.__dispatcher = dispatcher
        self.dependencies_resolver.register(
            DependencyRegistration(AbstractDispatcher, self.__dispatcher, is_instance=True)
        )
        return self

    def handle_generator(self, request: "Message[AsyncIterable[ResponseType]]") -> "AsyncIterable[ResponseType]":
        handler = self.__get_handler(request)
        return cast(AsyncIterable, handler(request))

    def handle(self, request: "Message[ResponseType]") -> "Awaitable[ResponseType]":
        handler = self.__get_handler(request)
        return handler(request)

    def get_logging_attributes(self) -> dict[str, str]:
        return {
            "service": self.name,
        }

    def __get_handler(self, message: Message):
        handler = self.__handlers.get(type(message))
        if not handler:
            raise NotImplementedError(
                f"No handler for {type(message)}, available: {[c.__name__ for c in self.__handlers.keys()]}"
            )
        return handler

    def __get_first_param_type(self, func: Callable[..., T]) -> type[T] | None:
        signature = inspect.signature(func)
        params = list(signature.parameters)
        if not params:
            return None
        return signature.parameters[params[0]].annotation

    @contextmanager
    def __events_collector(self):
        try:
            collector = None
            if self.event_bus:
                collector = self.event_bus.begin()
            yield collector
        except Exception as e:
            raise e
        else:
            if not collector:
                return

            if self.event_bus:
                self.event_bus.process(collector)

    async def __resolve_dependencies(self, stack: AsyncExitStack, func: Callable[P, T]):
        known_dependencies: dict = {
            AbstractDispatcher: self.dispatcher,
        }
        collector = self.__events_collector()
        if collector:
            known_dependencies[EventCollector] = collector
        known_dependencies = self.dependencies_resolver.resolve_dependencies(
            func, known_dependencies=known_dependencies
        )
        for param_name, param in known_dependencies.items():
            if isinstance(param, ContextManager):
                known_dependencies[param_name] = stack.enter_context(param)
            elif isinstance(param, AsyncContextManager):
                known_dependencies[param_name] = await stack.enter_async_context(param)
        return known_dependencies

    def __request_with_context(self, func: Callable[P, T]) -> Callable[P, Awaitable[T]]:
        async def injected_request_handler(*args: P.args, **kwargs: P.kwargs):
            async with AsyncExitStack() as stack:
                dependencies = await self.__resolve_dependencies(stack, func)
                response = await self.dispatcher.dispatch(func, *args, **kwargs, **dependencies)
                return response

        async def injected_stream_handler(*args: P.args, **kwargs: P.kwargs):
            async with AsyncExitStack() as stack:
                casted_func = cast(Callable[P, AsyncIterable[T]], func)
                dependencies = await self.__resolve_dependencies(stack, casted_func)
                generator = casted_func(*args, **kwargs, **dependencies)  # type: ignore
                gen: AsyncGenerator = await stack.enter_async_context(aclosing(generator))
                async for response in gen:
                    yield response

        return injected_stream_handler if inspect.isasyncgenfunction(func) else injected_request_handler  # type: ignore

    def __event_with_context(self, func: Callable[P, T]) -> Callable[P, None]:
        async def injected_event_handler(*args: P.args, **kwargs: P.kwargs):
            async with AsyncExitStack() as stack:
                dependencies = await self.__resolve_dependencies(stack, func)
                response = func(*args, **kwargs, **dependencies)
                if inspect.iscoroutine(response):
                    return await response
                return response

        def schedule(*args: P.args, **kwargs: P.kwargs):
            self.dispatcher.schedule(injected_event_handler, *args, **kwargs)

        return schedule

    def __delegate_subscriptions(self, event_bus: EventBus):
        if not self.__subscriptions:
            return

        while self.__subscriptions:
            event_type, handler = self.__subscriptions.pop(0)
            event_bus.subscribe(event_type, handler)

    def __register_handler(
        self,
        handler: "RequestHandler[RequestType, P, ResponseType] | EventHandler[EventType, P]",
    ) -> InjectedRequestHandler[RequestType, ResponseType] | EventBusHandler[EventType]:
        param_type = self.__get_first_param_type(handler)
        if not inspect.isclass(param_type):
            raise ValueError(f"Handler param type {param_type} must be a subclass of Message or Event")

        if issubclass(param_type, Message):
            injected_handler = self.__request_with_context(cast(RequestHandler[RequestType, P, ResponseType], handler))
            self.__handlers[param_type] = injected_handler
        elif issubclass(param_type, Event):
            injected_handler = self.__event_with_context(cast(EventHandler[EventType, P], handler))
            self.__subscriptions.append((param_type, injected_handler))
        else:
            raise ValueError(f"Handler param type {param_type} must be a subclass of Message or Event")

        return injected_handler
