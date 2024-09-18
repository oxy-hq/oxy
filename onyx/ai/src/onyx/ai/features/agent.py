from typing import AsyncIterable

from langchain_core.callbacks.base import BaseCallbackHandler
from langchain_core.runnables.config import RunnableConfig
from onyx.ai.adapters.tracing import AbstractTracingClient
from onyx.ai.base.builder import AbstractChainBuilder
from onyx.ai.base.callbacks import ConsoleCallbackHandler
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Message
from onyx.shared.models.common import AgentInfo, ChatContext, ChatMessage, StreamingChunk, StreamingTrace

logger = get_logger(__name__)


class StreamRequest(Message[AsyncIterable[StreamingChunk | StreamingTrace]]):
    text: str
    context: ChatContext
    chat_history: list[ChatMessage]
    agent_info: AgentInfo
    cite_sources: bool
    tracing_session_id: str | None


async def stream(
    request: StreamRequest,
    chain_builder: AbstractChainBuilder,
    tracer: AbstractTracingClient,
):
    chain = chain_builder.build(
        training_prompts=request.agent_info.training_prompts,
        data_sources=request.agent_info.data_sources,
        cite_sources=request.cite_sources,
    )
    callbacks: list[BaseCallbackHandler] = [ConsoleCallbackHandler()]
    should_trace = bool(request.tracing_session_id)

    if should_trace:
        trace_handler = tracer.get_langchain_handler(
            user_id=request.context.user_email,
            session_id=str(request.tracing_session_id),
        )

        if trace_handler:
            callbacks.append(trace_handler)
        else:
            logger.warning("Tracing handler not found")

    chain_config: RunnableConfig = {
        "callbacks": [],
    }

    try:
        async for chunk in chain.astream(
            {
                "message": request.text,
                "username": request.context.username,
                "agent_info": request.agent_info,
                "chat_history": request.chat_history,
            },
            config=chain_config,
        ):
            yield chunk
    finally:
        if should_trace and trace_handler:
            tracer.flush(trace_handler)
            trace_id = tracer.get_trace_id(trace_handler)
            trace_url = tracer.get_trace_url(trace_handler)
            if trace_id and trace_url:
                yield StreamingTrace(
                    trace_url=trace_url,
                    trace_id=trace_id,
                    total_duration=tracer.get_total_duration(trace_handler),
                    time_to_first_token=tracer.get_time_to_first_token(trace_handler),
                )
