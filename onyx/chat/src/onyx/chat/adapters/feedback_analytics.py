from abc import ABC, abstractmethod

from langfuse import Langfuse
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged


class AbstractFeedbackAnalytics(ABC):
    @abstractmethod
    def score(self, score: int, id: str, trace_id: str, comment: str):
        pass

    def delete_score(self, id: str, trace_id: str, comment: str):
        self.score(score=0, id=id, trace_id=trace_id, comment=comment)


class LangfuseFeedbackAnalytics(AbstractFeedbackAnalytics):
    STREAM_NAME = "user_feedback"

    def __init__(self, config: OnyxConfig):
        self.langfuse = Langfuse(
            secret_key=config.langfuse.secret_key,
            public_key=config.langfuse.public_key,
            host=config.langfuse.host,
        )

    def score(self, score: int, id: str, trace_id: str, comment: str):
        self.langfuse.score(
            # langfuse will upsert the score based on id and trace_id
            # this limit 1 feedback per user
            name=self.STREAM_NAME,
            value=score,
            id=id,
            trace_id=trace_id,
            comment=comment,
        )


class ConsoleFeedbackAnalytics(Logged, AbstractFeedbackAnalytics):
    def score(self, score: int, id: str, trace_id: str, comment: str):
        self.log.info(f"Score: {score}, ID: {id}, Trace ID: {trace_id}, Comment: {comment}")
