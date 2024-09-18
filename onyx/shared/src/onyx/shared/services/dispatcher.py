import asyncio
import inspect
from abc import ABC, abstractmethod
from concurrent.futures import ThreadPoolExecutor
from functools import partial
from typing import Any, Awaitable, Callable, ParamSpec, TypeVar

from onyx.shared.logging import Logged
from typing_extensions import TypedDict

P = ParamSpec("P")
R = TypeVar("R")


class MapParam(TypedDict, total=False):
    args: list[Any]
    kwargs: dict[Any, Any]


class AbstractDispatcher(ABC):
    @abstractmethod
    def dispatch(self, func: Callable[P, R], *args: P.args, **kwargs: P.kwargs) -> Awaitable[R]:
        pass

    @abstractmethod
    def schedule(self, func: Callable[P, R], *args: P.args, **kwargs: P.kwargs) -> None:
        pass

    @abstractmethod
    def map(self, func: Callable[P, R], params: list[MapParam]) -> Awaitable[list[R]]:
        pass


class AsyncIODispatcher(Logged, AbstractDispatcher):
    def __init__(self, graceful_shutdown_timeout: int = 5, max_workers: int = 100):
        self.__loop: asyncio.AbstractEventLoop | None = None
        self.__futures = set[asyncio.Future]()
        self.__pool: ThreadPoolExecutor = ThreadPoolExecutor(max_workers=max_workers)
        self.graceful_shutdown_timeout = graceful_shutdown_timeout

    @property
    def loop(self) -> asyncio.AbstractEventLoop:
        if self.__loop is None:
            self.__loop = asyncio.get_event_loop()
        return self.__loop

    def dispatch(self, func: Callable[P, R], *args: P.args, **kwargs: P.kwargs) -> asyncio.Future[R]:
        if inspect.iscoroutinefunction(func):
            return self.loop.create_task(func(*args, **kwargs))

        future = self.loop.run_in_executor(self.__pool, partial(func, *args, **kwargs))
        return future

    def schedule(self, func: Callable[P, R], *args: P.args, **kwargs: P.kwargs) -> None:
        self.__watch_future(self.dispatch(func, *args, **kwargs))

    def map(self, func: Callable[P, R], params: list[MapParam]) -> asyncio.Future[list[R]]:
        return asyncio.gather(*[self.dispatch(func, *param["args"], **param["kwargs"]) for param in params])  # type: ignore

    async def teardown(self) -> None:
        try:
            async with asyncio.timeout(self.graceful_shutdown_timeout):
                await asyncio.gather(*self.__futures)
        except TimeoutError:
            self.log.warning(f"Graceful shutdown timed out, remaining tasks {len(self.__futures)}")

        for future in self.__futures:
            future.cancel()

    def __watch_future(self, future: asyncio.Future) -> None:
        self.__futures.add(future)
        future.add_done_callback(self.__unwatch_future)

    def __unwatch_future(self, future: asyncio.Future) -> None:
        self.__futures.discard(future)
        try:
            exc = future.exception()
            if exc:
                self.log.error(f"Unhandled exception in task {exc}", exc_info=True)
            else:
                self.log.info(f"Task completed. Remaining tasks: {len(self.__futures)}")
        except asyncio.CancelledError:
            self.log.warning(f"Task was cancelled {future}")
            pass
