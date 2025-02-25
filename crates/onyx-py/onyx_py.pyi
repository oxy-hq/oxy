from dataclasses import dataclass
from typing import TypedDict

from pyarrow import RecordBatch

@dataclass
class Task:
    name: str
    output: str

@dataclass
class AgentResult:
    output: str
    tasks: list[Task]

@dataclass
class WorkflowResultTask:
    ...


WorkflowOutput = str | dict[str, WorkflowOutput] | list[WorkflowOutput] | list[RecordBatch]

@dataclass
class WorkflowResult:
    output: WorkflowOutput
    tasks: WorkflowResultTask

class RunOptions(TypedDict):
    question: str | None
    variables: list[tuple[str, str]] | None
    database: str | None

RunOutput = str | AgentResult | WorkflowResult

def run(file: str, options: RunOptions ) -> str:
    ...