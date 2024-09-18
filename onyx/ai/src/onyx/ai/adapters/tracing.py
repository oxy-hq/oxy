import logging
from abc import ABC, abstractmethod

from langchain.callbacks.base import BaseCallbackHandler
from langfuse.api.resources.commons import TraceWithFullDetails
from langfuse.callback.langchain import LangchainCallbackHandler
from langfuse.client import Langfuse
from onyx.ai.adapters.custom_langfuse_callback_handler import CustomLangfuseCallbackHandler
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged


class AbstractTracingClient(ABC):
    @abstractmethod
    def get_langchain_handler(self, user_id: str, session_id: str) -> BaseCallbackHandler | None:
        ...

    @abstractmethod
    def get_trace_id(self, handler: BaseCallbackHandler) -> str | None:
        ...

    @abstractmethod
    def get_trace_url(self, handler: BaseCallbackHandler) -> str | None:
        ...

    @abstractmethod
    def get_total_duration(self, handler: BaseCallbackHandler) -> float | None:
        ...

    @abstractmethod
    def flush(self, handler):
        ...

    @abstractmethod
    def get_time_to_first_token(self, handler: BaseCallbackHandler) -> float | None:
        ...


class LangfuseTracingClient(AbstractTracingClient):
    def __init__(self, config: OnyxConfig):
        self.langfuse = Langfuse(
            enabled=config.langfuse.enabled,
            public_key=config.langfuse.public_key,
            secret_key=config.langfuse.secret_key,
            host=config.langfuse.host,
        )

    def __get_trace(self, handler: BaseCallbackHandler):
        if isinstance(handler, LangchainCallbackHandler):
            return handler.trace

    def get_langchain_handler(self, user_id: str, session_id: str) -> BaseCallbackHandler | None:
        if self.langfuse.enabled is False:
            return None

        trace = self.langfuse.trace(user_id=user_id, session_id=session_id)
        try:
            trace.log.debug(f"Creating new handler for trace {trace.id}")
            return CustomLangfuseCallbackHandler(
                stateful_client=trace,
                debug=trace.log.level == logging.DEBUG,
                update_stateful_client=False,
            )
        except Exception as e:
            trace.log.exception(e)
        return trace.get_langchain_handler()

    def get_trace_id(self, handler: BaseCallbackHandler) -> str | None:
        trace = self.__get_trace(handler)
        return trace and trace.id

    def get_trace_url(self, handler: BaseCallbackHandler) -> str | None:
        trace = self.__get_trace(handler)
        return trace and trace.get_trace_url()

    def get_total_duration(self, handler: BaseCallbackHandler) -> float | None:
        trace = self.__get_trace(handler)
        trace_with_details = trace.client.trace.get(trace_id=trace.trace_id)
        return round(trace_with_details.latency, 2)

    def __calculate_time_to_first_token(self, trace: TraceWithFullDetails):
        if len(trace.observations) == 0:
            return None
        root_observation = trace.observations[0]
        answer_builder_observation = trace.observations[-1]
        if answer_builder_observation.time_to_first_token is None:
            return None
        rs = (
            answer_builder_observation.start_time - root_observation.start_time
        ).total_seconds() + answer_builder_observation.time_to_first_token
        return round(rs, 2)

    def get_time_to_first_token(self, handler: BaseCallbackHandler) -> float | None:
        trace = self.__get_trace(handler)
        trace_with_details = trace.client.trace.get(trace_id=trace.trace_id)
        return self.__calculate_time_to_first_token(trace_with_details)

    def flush(self, handler):
        if isinstance(handler, LangchainCallbackHandler):
            handler.flush()


class NoopTracingClient(Logged, AbstractTracingClient):
    def get_langchain_handler(self, user_id: str, session_id: str) -> BaseCallbackHandler | None:
        return None

    def get_trace_id(self, handler: BaseCallbackHandler) -> str | None:
        return None

    def get_trace_url(self, handler: BaseCallbackHandler) -> str | None:
        return None

    def get_total_duration(self, handler: BaseCallbackHandler) -> float | None:
        return None

    def get_time_to_first_token(self, handler: BaseCallbackHandler) -> float | None:
        return None

    def flush(self, handler):
        return
