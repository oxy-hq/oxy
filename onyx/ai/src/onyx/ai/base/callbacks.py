from typing import Any

from langchain.callbacks.tracers import FunctionCallbackHandler
from onyx.shared.logging import Logged


class ConsoleCallbackHandler(Logged, FunctionCallbackHandler):
    """Tracer that prints to the console."""

    name: str = "console_callback_handler"

    def __init__(self, **kwargs: Any) -> None:
        super().__init__(function=self.log.debug, **kwargs)
