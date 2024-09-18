from collections import defaultdict
from typing import Any, Callable, TypeVar

from onyx.shared.logging import Logged
from onyx.shared.models.base import Command, Event
from onyx.shared.models.handlers import EventBusHandler
from onyx.shared.services.unit_of_work import AbstractUnitOfWork

CommandType = TypeVar("CommandType", bound=Command)
EventType = TypeVar("EventType", bound=Event)
MessageType = CommandType | EventType
Handler = Callable[[MessageType], Any]
Message = Command | Event
UnitOfWorkType = TypeVar("UnitOfWorkType", bound=AbstractUnitOfWork)


class EventBus(Logged):
    """
    The `EventBus` class provides a simple event bus implementation that allows
    components to subscribe to and publish events. It supports multiple event types
    and handlers.

    Usage:
    bus = EventBus()
    bus.subscribe(EventType, handler)
    bus.publish(EventType())
    bus.process()
    """

    def __init__(self):
        self.__subscriptions: "dict[type, list[EventBusHandler]]" = defaultdict(list)

    def begin(self):
        return EventCollector()

    def subscribe(self, event_type: type, handler: EventBusHandler):
        self.__subscriptions[event_type].append(handler)

    def process(self, collector: "EventCollector"):
        for event in collector.collect():
            handlers = self.__subscriptions[type(event)]
            self.log.debug(f"Processing event {repr(event)} with {handlers}")
            for handler in handlers:
                try:
                    handler(event)
                except Exception:
                    self.log.error(f"Error handling event {repr(event)}", exc_info=True)
                    continue


class EventCollector(Logged):
    def __init__(self) -> None:
        self.__events: list[Event] = []

    def publish(self, *events: Event):
        self.__events.extend(events)

    def collect(self):
        while self.__events:
            yield self.__events.pop(0)
