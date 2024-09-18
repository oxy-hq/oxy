import asyncio
import time
from typing import Any, Iterable

import pytest
from onyx.shared.services.dispatcher import AsyncIODispatcher


async def resolve_race(tasks: Iterable[asyncio.Future[Any]]):
    results = []
    completed, pending = await asyncio.wait(
        tasks,
        return_when=asyncio.FIRST_COMPLETED,
    )
    for task in completed:
        results.append(task.result())

    if pending:
        results.extend(await resolve_race(pending))

    return results


@pytest.mark.asyncio(scope="session")
async def test_dispatch_should_not_wait_for_schedule():
    dispatcher = AsyncIODispatcher()
    results: list[int] = []

    async def scheduler():
        await asyncio.sleep(0.01)
        results.append(1)

    def handler():
        return 2

    dispatcher.schedule(scheduler)
    response = await dispatcher.dispatch(handler)
    assert response == 2
    await asyncio.sleep(0.01)
    assert results == [1]


@pytest.mark.asyncio(scope="session")
async def test_dispatch_blocking_async():
    dispatcher = AsyncIODispatcher()

    async def blocking():
        # This represents a blocking coroutine that takes a long time to finish
        # something like a database query or a network request in non-asyncio styles.
        # Don't mix async and non-async code in the same coroutine.
        time.sleep(0.02)
        return 1

    def blocking_correct_way():
        time.sleep(0.02)
        return 4

    async def non_blocking():
        await asyncio.sleep(0.01)
        return 2

    def non_blocking_sync():
        return 3

    results = await resolve_race(
        [
            dispatcher.dispatch(blocking),
            dispatcher.dispatch(non_blocking),
        ]
    )
    assert results == [1, 2]

    results = await resolve_race(
        [
            dispatcher.dispatch(blocking_correct_way),
            dispatcher.dispatch(non_blocking_sync),
        ]
    )
    assert results == [3, 4]
