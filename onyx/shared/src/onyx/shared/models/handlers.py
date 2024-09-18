from dataclasses import dataclass
from typing import Any, Awaitable, Callable, Concatenate, Generic, Iterable, ParamSpec, TypeVar

from onyx.app import AbstractOnyxApp
from onyx.shared.config import OnyxConfig
from onyx.shared.models.base import Event, Message
from punq import Scope

RequestType = TypeVar("RequestType", bound=Message)
ResponseType = TypeVar("ResponseType")
EventType = TypeVar("EventType", bound=Event)
P = ParamSpec("P")
RequestHandler = Callable[Concatenate[RequestType, P], ResponseType]
InjectedRequestHandler = Callable[[RequestType], Awaitable[ResponseType]]
EventHandler = Callable[Concatenate[EventType, P], Any]
EventBusHandler = Callable[[EventType], None]
T = TypeVar("T")


@dataclass
class DependencyRegistration(Generic[T]):
    dependency_type: type[T]
    dependency: T | Callable[..., T]
    is_instance: bool = False
    scope: Scope = Scope.transient


ConfigMapper = Callable[[OnyxConfig, AbstractOnyxApp], Iterable[DependencyRegistration[T]]]
