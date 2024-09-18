from enum import IntEnum, StrEnum, auto


class ConnectionSlugChoices(StrEnum):
    snowflake = auto()
    bigquery = auto()
    clickhouse = auto()


class IntegrationSlugChoices(StrEnum):
    salesforce = auto()
    gmail = auto()
    slack = auto()
    notion = auto()
    file = auto()


class ConnectionSyncStatus(StrEnum):
    initial = "initial"
    syncing = "syncing"
    error = "error"
    success = "success"


def map_legacy_state(status: str | None) -> "DagState":
    if status == ConnectionSyncStatus.initial:
        return DagState.QUEUED
    elif status == ConnectionSyncStatus.syncing:
        return DagState.RUNNING
    elif status == ConnectionSyncStatus.success:
        return DagState.SUCCESS
    elif status == ConnectionSyncStatus.error:
        return DagState.FAILED
    return DagState.SUCCESS


class TaskQueueSystems(StrEnum):
    airflow = auto()


class DataSourceType(StrEnum):
    warehouse = auto()
    integration = auto()


class DagState(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    SUCCESS = "success"
    FAILED = "failed"


class ChainTypes(IntEnum):
    DEFAULT = 0
    AGENT = auto()
    REWOO = auto()


DEFAULT_NAMESPACE = "default"
FEATURED_CATEGORY = "Featured"
EnvConfigType = dict[str, str]
