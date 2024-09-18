from onyx.shared.models.base import Event
from onyx.shared.services.message_bus import EventBus


def test_process_event():
    bus = EventBus()
    event_collector = bus.begin()

    class DummyEvent(Event):
        count: int

    def dummy_handler(event: DummyEvent):
        event.count += 1

    bus.subscribe(DummyEvent, dummy_handler)
    dummy_event = DummyEvent(count=0)
    event_collector.publish(dummy_event)

    bus.process(event_collector)
    assert dummy_event.count == 1
