from abc import ABC
from asyncio import gather
from typing import cast

from dspy import Example, Module, Predict, Prediction, settings, signature_to_template
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import (
    AIMessage,
    AIMessageChunk,
    BaseMessage,
    BaseMessageChunk,
    HumanMessage,
    SystemMessage,
    ToolCall,
    ToolMessage,
)
from langchain_core.runnables import Runnable
from onyx.ai.base.builder import ChainInputWithContext
from onyx.ai.base.citation import CitationMarker, CitationState
from onyx.ai.base.tools import ToolsRegistry
from onyx.shared.logging import Logged
from onyx.shared.models.common import AgentInfo, ChatMessage, Step, StreamingChunk
from slugify import slugify


class StreamPredictMeta(type(ABC), type(Module)):
    ...


class LangchainStreamPredict(
    Logged, Runnable[ChainInputWithContext, StreamingChunk], Predict, metaclass=StreamPredictMeta
):
    def __init__(
        self,
        langchain_llm: BaseChatModel,
        signature,
        tools: ToolsRegistry,
        citation_marker: CitationMarker | None = None,
        max_depth: int = 5,
        **config,
    ):
        super().__init__(signature, **config)
        self.langchain_llm = langchain_llm
        self.citation_marker = citation_marker
        self.output_field_key = list(self.signature.model_fields.keys())[-1]
        self.template = signature_to_template(self.signature)
        self.tools: ToolsRegistry = tools
        self.max_depth = max_depth

    def __deserialize_messages(
        self, messages: list[ChatMessage], system_message: str, username: str, agent_info: AgentInfo
    ) -> list[BaseMessage]:
        response: list[BaseMessage] = [
            SystemMessage(
                content=system_message,
                name="AGENT",
            )
        ]
        for message in messages:
            if not message.content:
                continue

            if message.is_ai_message:
                response.append(
                    AIMessage(
                        content=message.content,
                        name=slugify(agent_info.name, separator="_"),
                    )
                )
            else:
                response.append(HumanMessage(content=message.content, name=slugify(username, separator="_")))
        return response

    async def __get_streaming_chunks(self, input: ChainInputWithContext):
        for key in list(input.keys()):
            if isinstance(input[key], StreamingChunk):
                yield input.pop(key)  # type: ignore

    async def atransform(
        self,
        input,
        config=None,
        **kwargs,
    ):
        final: ChainInputWithContext
        got_first_val = False
        async for ichunk in input:
            async for value in self.__get_streaming_chunks(ichunk):
                yield value

            if not got_first_val:
                final = ichunk
                got_first_val = True
            else:
                try:
                    final = final + ichunk  # type: ignore[operator]
                except TypeError:
                    final = ichunk

        if got_first_val:
            async for output in self.astream(final, config, **kwargs):
                yield output

    async def _execute_tools(self, calls: list[ToolCall], depth: int):
        if depth > self.max_depth:
            return [
                ToolMessage(tool_call_id=c["id"], name=c["name"], content=f"Max depth of {self.max_depth} reached")
                for c in calls
            ]
        coros = [self.tools.get(c["name"]).run(c["args"]) for c in calls]  # type: ignore
        results = await gather(*coros)
        return [ToolMessage(tool_call_id=c["id"], name=c["name"], content=r) for c, r in zip(calls, results) if c["id"]]

    async def _execute(self, messages: list[BaseMessage], config, depth=1):
        tools = self.tools.to_spec()
        self.log.debug(f"Provided tools: {tools}")
        if depth > self.max_depth + 1:
            raise ValueError(f"Max depth of {self.max_depth} reached")

        generation: BaseMessageChunk | None = None
        tool_kwargs = {"tools": tools} if tools else {}
        async for chunk in self.langchain_llm.astream(messages, config, **tool_kwargs):
            if generation is None:
                generation = chunk
            else:
                generation = generation + chunk

            yield chunk

        if isinstance(generation, AIMessageChunk) and generation.tool_calls:
            tool_messages = await self._execute_tools(generation.tool_calls, depth)
            self.log.info(f"Tool Results: {tool_messages}")
            messages.append(generation)
            async for chunk in self._execute([*messages, *tool_messages], config, depth + 1):
                yield chunk

    async def astream(
        self,
        input,
        config=None,
        **kwargs,
    ):
        yield StreamingChunk.step(Step.GenerateAnswer)
        prompt = self.template(
            Example(
                demos=self.demos,
                **input,
            ),
            show_guidelines=False,
        )
        messages = self.__deserialize_messages(input["chat_history"], prompt, input["username"], input["agent_info"])
        self.log.debug(f"Messages History: {messages}")
        citation_state = CitationState()

        async for chunk in self._execute(messages, config):
            content = cast(str, chunk.content)

            if not self.citation_marker:
                # citation is disabled, just yield the content
                yield StreamingChunk.content(content)
                continue

            if ":" not in content and citation_state.is_empty():
                yield StreamingChunk.content(content)
                continue

            for char in content:
                # process citation if it's not disabled
                result = citation_state.process(char)
                if result:
                    marked_content, sources = self.citation_marker.mark_used(result)
                    yield StreamingChunk.content(text=marked_content, sources=sources)

    def forward(self, input, _config=None, **kwargs):
        signature = kwargs.pop("signature", self.signature)
        demos = kwargs.pop("demos", self.demos)
        template = signature_to_template(signature)

        prompt = template(Example(demos=demos, **input))
        output = self.langchain_llm.invoke(prompt, _config, **kwargs)
        try:
            content = output.content
        except AttributeError:
            content = output

        content, sources = content, []
        if self.citation_marker:
            content, sources = self.citation_marker.mark_used(cast(str, content))
        pred = Prediction(
            **{
                self.output_field_key: content,
                "sources": [x.label for x in sorted(sources, key=lambda x: x.number)],
            }
        )
        self.log.info(f"input: {input['message']} content: {pred}")

        cast(list, settings.langchain_history).append((prompt, pred))

        if settings.trace is not None:
            trace = settings.trace
            trace.append((self, {**input}, pred))

        return pred

    def invoke(
        self,
        input,
        config=None,
        **kwargs,
    ):
        return self.forward(input, config, **kwargs)
