import asyncio
from abc import ABC, abstractmethod

from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from slack_sdk.errors import SlackApiError
from slack_sdk.web.async_client import AsyncWebClient


class AbstractNotification(ABC):
    @abstractmethod
    async def notify_generated(
        self,
        *,
        user_email: str,
        question: str,
        answer: str,
        trace_url: str | None,
        agent_name: str,
        total_duration: float | None,
        time_to_first_token: float | None,
    ) -> None:
        pass

    @abstractmethod
    async def notify_searched(self, *, user_email: str, query: str, result: list) -> str | None:
        pass

    @abstractmethod
    async def notify_previewed(
        self,
        *,
        agent_name: str,
        content: str,
        slack_thread_ts: str,
        trace_url: str | None,
        total_duration: float | None,
        time_to_first_token: float | None,
    ) -> None:
        pass


class ConsoleNotification(Logged, AbstractNotification):
    async def notify_generated(
        self,
        *,
        user_email: str,
        question: str,
        answer: str,
        trace_url: str | None,
        agent_name: str,
        total_duration: float | None,
        time_to_first_token: float | None,
    ) -> None:
        self.log.info(f"User: {user_email}")
        self.log.info(f"Agent: {agent_name}")
        self.log.info(f"Question: {question}")
        self.log.info(f"Answer: {answer}")
        self.log.info(f"Trace URL: {trace_url}")
        self.log.info(f"Total duration: {total_duration}")
        self.log.info(f"Time to first token: {time_to_first_token}")

    async def notify_searched(self, *, user_email: str, query: str, result: list) -> str | None:
        self.log.info(f"User: {user_email}")
        self.log.info(f"Query: {query}")
        self.log.info(f"Result: {result}")
        return "(example) slack_thread_ts"

    async def notify_previewed(
        self,
        *,
        agent_name: str,
        content: str,
        slack_thread_ts: str,
        trace_url: str | None,
        total_duration: float | None,
        time_to_first_token: float | None,
    ) -> None:
        self.log.info(f"Agent: {agent_name}")
        self.log.info(f"Content: {content}")
        self.log.info(f"Slack thread timestamp: {slack_thread_ts}")
        self.log.info(f"Trace url: {trace_url}")
        self.log.info(f"Total duration: {total_duration}")
        self.log.info(f"Time to first token: {time_to_first_token}")


class SlackNotification(Logged, AbstractNotification):
    def __init__(self, config: OnyxConfig):
        self.client = AsyncWebClient(token=config.slack.bot_token)
        self.channel = config.slack.qa_log_channel
        self.search_channel = config.slack.search_log_channel

    def __format_message(
        self,
        user_email: str,
        question: str,
        answer: str,
        agent_name: str,
        trace_url: str | None = None,
        total_duration: float | None = None,
        time_to_first_token: float | None = None,
    ) -> str:
        trace_url = trace_url if trace_url else "No trace URL available for cached response"
        message = f"""
-------------
User: `{user_email}`

Agent: `{agent_name}`

Question:
```
{question}
```

Answer:
```
{answer}
```
Time to first token: {time_to_first_token}s
Total duration: {total_duration}s
Trace URL: {trace_url}
-------------
"""
        return message

    async def notify_generated(
        self,
        *,
        user_email: str,
        question: str,
        answer: str,
        trace_url: str | None,
        agent_name: str,
        total_duration: float | None,
        time_to_first_token: float | None,
    ) -> None:
        message = self.__format_message(
            user_email, question, answer, agent_name, trace_url, total_duration, time_to_first_token
        )
        asyncio.create_task(self.client.chat_postMessage(channel=self.channel, text=message))

    async def notify_searched(self, *, user_email: str, query: str, result: list) -> str | None:
        message = f"""
-------------
New search!
User: `{user_email}`
Query: `{query}`
Results: `{result}`
-------------
"""
        try:
            resp = await self.client.chat_postMessage(channel=self.search_channel, text=message)
            if resp["ok"]:
                return resp["ts"]
        except SlackApiError as e:
            self.log.error(f"Failed to send notification: {e.response['error']}")
        return None

    async def notify_previewed(
        self,
        *,
        agent_name: str,
        content: str,
        slack_thread_ts: str,
        trace_url: str | None,
        total_duration: str | None,
        time_to_first_token: str | None,
    ) -> None:
        message = f"""
*{agent_name}*:

```
{content}
```
"""
        if time_to_first_token:
            message += f"\nTime to first token: {time_to_first_token}s"
        if total_duration:
            message += f"\nTotal duration: {total_duration}s"
        if trace_url:
            message += f"\nTrace URL: {trace_url}"

        # fire and forget
        asyncio.create_task(
            self.client.chat_postMessage(channel=self.search_channel, text=message, thread_ts=slack_thread_ts)
        )
