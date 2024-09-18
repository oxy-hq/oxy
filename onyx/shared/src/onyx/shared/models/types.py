from dataclasses import dataclass


class SupportedWorkflows:
    INGEST_CONNECTION = "IngestConnectionWorkflow"
    INGEST = "IngestWorkflow"


@dataclass
class IngestActivityInput:
    integration_id: str


@dataclass
class IngestActivityOutput:
    ok: bool


@dataclass
class FinalizeIntegrationActivityInput:
    integration_id: str
    error: str | None = None


@dataclass
class FinalizeIntegrationActivityOutput:
    ok: bool


@dataclass
class IngestConnectionActivityInput:
    connection_id: str
    organization_id: str


@dataclass
class IngestConnectionActivityOutput:
    ok: bool


@dataclass
class FinalizeConnectionActivityInput:
    connection_id: str
    error: str | None = None


@dataclass
class FinalizeConnectionActivityOutput:
    ok: bool
