import abc
from datetime import datetime, timezone
from typing import Literal, NamedTuple, Union
from uuid import uuid4

from onyx.catalog.adapters.airflow import AirflowClient
from onyx.catalog.adapters.pipeline.env_manager import AbstractPipelineEnvManager
from onyx.catalog.models.integration import Integration
from onyx.catalog.models.task import Task
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from onyx.shared.models.constants import (
    DagState,
    TaskQueueSystems,
)


class TaskResult(NamedTuple):
    id: str
    state: Union[DagState, Literal["PENDING", "RETRY", "SUCCESS", "FAILURE", "REVOKED"]]
    date_done: datetime | None


class AbstractTaskQueuePublisher(Logged, abc.ABC):
    def __init__(self, system_name):
        self.__system_name = system_name

    @abc.abstractmethod
    def publish_integration_created(self, integration: Integration) -> Task:
        ...

    @abc.abstractmethod
    def get_task_result_by_id(self, external_id: str, slug: str) -> TaskResult:
        ...

    @abc.abstractmethod
    def is_task_running(self, external_id: str, slug: str) -> bool:
        ...

    def get_logging_attributes(self) -> dict[str, str]:
        attrs = super().get_logging_attributes()
        return {
            **attrs,
            "queue_system": self.__system_name,
        }


class AirflowPublisher(AbstractTaskQueuePublisher):
    system_name: TaskQueueSystems = TaskQueueSystems.airflow
    MANUAL_DAG_NAME_PREFIX = "meltano_manual_"

    def __init__(
        self,
        config: OnyxConfig,
        env_manager: AbstractPipelineEnvManager,
    ) -> None:
        super().__init__(self.system_name)
        self.__airflow_app = AirflowClient(config.airflow)
        self.__env_manager = env_manager

    def publish_integration_created(self, integration: Integration) -> Task:
        if not integration.id:
            raise ValueError("Integration ID is required to publish task")

        payload = {
            "args": (
                str(integration.organization_id),
                str(integration.id),
            )
        }
        logical_date = datetime.now(tz=timezone.utc)
        task_id = str(uuid4())
        task = Task(
            id=task_id,
            queue_system=self.system_name,
            source_type="integration",
            execution_type="manual",
            source_id=integration.id,
            request_payload=payload,
            external_id=task_id,
        )
        ingest_env = self.__env_manager.get_ingest_env(integration)
        embed_env = self.__env_manager.get_embed_env(integration)
        response = self.__airflow_app.trigger_dag_run(
            f"{self.MANUAL_DAG_NAME_PREFIX}{integration.slug}",
            f"{task_id}",
            {
                **ingest_env,
                **embed_env,
            },
            logical_date,
        )
        self.log.info("response: %s", response, integration_id=integration.id)

        return task

    def get_task_result_by_id(self, external_id: str, slug: str):
        dag_id = f"{self.MANUAL_DAG_NAME_PREFIX}{slug}"

        task_result = self.__airflow_app.get_dag_run(dag_id=str(dag_id), dag_run_id=external_id)
        if task_result is None:
            return TaskResult(
                id=external_id,
                state=DagState.FAILED,
                date_done=None,
            )

        state = str(task_result["state"])
        date_done = None

        if state == DagState.SUCCESS:
            date_done = task_result.end_date

        return TaskResult(
            id=external_id,
            state=state,
            date_done=date_done,
        )

    def is_task_running(self, external_id: str, slug: str) -> bool:
        task_result = self.get_task_result_by_id(external_id, slug)
        return task_result.state in {
            DagState.RUNNING,
            task_result.state == DagState.QUEUED,
        }
