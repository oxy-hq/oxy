"""Event sinks for the pipeline.

`print`  — log the detected upsell (default; for eval / dry runs).
`oxy`    — POST to the control plane `/control/events` (same endpoint + body
           `{"events":[…]}` the edge outbox uses), so events land in
           `oxy_cam_events` for the rollup.

On the edge box the production path rides the worker's existing authenticated
outbox (device JWT). This standalone poster feeds test events from a consented
recording; it reads the control-plane URL + a bearer token from the env:

    OXY_CONTROL_URL   e.g. https://<box>.oxy…  (control-plane base)
    OXY_DEVICE_TOKEN  the box's device JWT

`only_item` is an optional per-item filter (e.g. to focus a run on avocado);
None = emit every upsell. The rollup is general — it reads the offered item
from the event's `label` and matches conversion per-item.
"""
from __future__ import annotations

import os
from collections.abc import Callable

Sink = Callable[..., None]


def make_sink(mode: str, *, only_item: str | None = None) -> Sink:
    base: Sink = _oxy_sink() if mode == "oxy" else _print_sink
    if not only_item:
        return base
    want = only_item.lower()

    def filtered(event: dict, *, evidence, at) -> None:
        if want in (event.get("label") or "").lower():
            base(event, evidence=evidence, at=at)
        else:
            print(f"    (skip: item {event.get('label')!r} != {only_item!r})")

    return filtered


def _print_sink(event: dict, *, evidence: str | None, at: float) -> None:
    print(f"\n>>> upsell_attempt @ ~{at:6.1f}s  item={event['label']!r} "
          f"conf={event['confidence']}")
    if evidence:
        print(f"    heard: «{evidence}»")
    print(f"    event: {event}")


def _oxy_sink() -> Sink:
    import httpx

    base_url = os.environ["OXY_CONTROL_URL"]
    token = os.environ["OXY_DEVICE_TOKEN"]
    client = httpx.Client(base_url=base_url, timeout=30.0,
                          headers={"Authorization": f"Bearer {token}"})

    def sink(event: dict, *, evidence: str | None, at: float) -> None:
        r = client.post("/control/events", json={"events": [event]})
        r.raise_for_status()
        print(f">>> posted upsell_attempt {event['event_id'][:8]} "
              f"item={event['label']!r} @~{at:.1f}s -> /control/events")

    return sink
