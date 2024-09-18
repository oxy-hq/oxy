import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Callable, Sequence

from onyx.shared.models.constants import (
    ConnectionSlugChoices,
    DataSourceType,
    IntegrationSlugChoices,
)
from pydantic import BaseModel, Field
from typing_extensions import TypedDict


class Column(TypedDict):
    name: str
    type: str


class Table(TypedDict):
    schema: str
    name: str
    columns: list[Column]


class DataSource(TypedDict):
    name: str
    database: str
    table: str
    slug: IntegrationSlugChoices | ConnectionSlugChoices
    type: DataSourceType
    organization_id: uuid.UUID
    id: uuid.UUID
    source_tables: list[Table]
    metadata: dict


class ExecuteQueryPayload(TypedDict):
    query: str
    organization_id: str
    id: str


ExecuteQueryFunc = Callable[[ExecuteQueryPayload], Sequence[Any]]


class TrainingPromptSource(TypedDict):
    id: str
    type: str
    filters: str
    target_embedding_table: str


class TrainingPrompt(TypedDict):
    message: str
    sources: list[TrainingPromptSource]


class AgentInfo(BaseModel):
    name: str
    instructions: str
    description: str
    knowledge: str
    data_sources: list[DataSource]
    training_prompts: list[TrainingPrompt]

    def to_prompt(self) -> str:
        return f"""---
Name: {self.name}
Description: {self.description}
Instruction: {self.instructions}
Knowledge: {self.knowledge}
---
"""


class AgentDisplayInfo(TypedDict):
    name: str
    subdomain: str
    avatar: str


class ChatMessage(BaseModel):
    content: str
    is_ai_message: bool


class ChatContext(BaseModel):
    organization_id: uuid.UUID | None
    username: str
    user_email: str
    user_id: uuid.UUID
    channel_id: uuid.UUID | None = None
    current_date: str = Field(default_factory=lambda: datetime.now().isoformat())


class Step(str, Enum):
    QueryGenerate = "query-generate"
    ChartGenerate = "chart-generate"
    FetchData = "fetch-data"
    Thinking = "thinking"
    GenerateAnswer = "generate-answer"


class Source(BaseModel):
    label: str
    content: str
    type: str
    number: int
    url: str = ""
    page: str = ""


class StreamingChunk(BaseModel):
    text: str
    sources: list[Source]
    steps: list[Step] = Field(default_factory=list)

    @classmethod
    def step(cls, step: Step) -> "StreamingChunk":
        return cls(text="", sources=[], steps=[step])

    @classmethod
    def content(cls, text: str, sources: list[Source] | None = None) -> "StreamingChunk":
        sources = sources or []
        return cls(text=text, sources=sources, steps=[])


class StreamingTrace(BaseModel):
    trace_id: str
    trace_url: str
    total_duration: float | None
    time_to_first_token: float | None
