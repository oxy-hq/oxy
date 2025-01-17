from dataclasses import dataclass
from typing import TypedDict

from pyarrow import RecordBatch

@dataclass
class Step:
    name: str
    output: str

@dataclass
class AgentResult:
    output: str
    steps: list[Step]

@dataclass
class WorkflowResultStep:
    ...


WorkflowOutput = str | dict[str, WorkflowOutput] | list[WorkflowOutput] | list[RecordBatch]

@dataclass
class WorkflowResult:
    output: WorkflowOutput
    steps: WorkflowResultStep

class RunOptions(TypedDict):
    question: str | None
    variables: list[tuple[str, str]] | None
    warehouse: str | None

RunOutput = str | AgentResult | WorkflowResult

def run(file: str, options: RunOptions ) -> str:
    ...