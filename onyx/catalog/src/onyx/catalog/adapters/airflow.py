import abc
from datetime import datetime

from airflow_client.client import (
    ApiClient,
    Configuration,
)
from airflow_client.client.api import dag_run_api
from airflow_client.client.exceptions import NotFoundException
from airflow_client.client.model.clear_dag_run import ClearDagRun
from airflow_client.client.model.dag_run import DAGRun
from onyx.shared.config import AirflowSettings
from onyx.shared.logging import Logged


class AbstractAirflowClient(abc.ABC):
    @abc.abstractmethod
    def trigger_dag_run(self, dag_id: str, dag_run_id: str, conf: dict, note: str):
        ...

    @abc.abstractmethod
    def get_dag_run(self, dag_id: str, dag_run_id: str):
        ...


class AirflowClient(Logged):
    def __init__(self, airflow: AirflowSettings):
        self.__configuration = Configuration(
            host=f"{airflow.web_server_url}/api/v1",
            username=airflow.user_name,
            password=airflow.password,
        )

        try:
            self.__api_client = ApiClient(self.__configuration)
            self.__api_instance = dag_run_api.DAGRunApi(self.__api_client)
            self.log.info("Init airflow client successfully")
        except Exception as e:
            self.log.error(f"Failed to init airflow client instance: {e}")

    def trigger_dag_run(self, dag_id: str, dag_run_id: str, conf: dict, logical_date: datetime):
        dag_run = DAGRun(
            dag_run_id=dag_run_id,
            logical_date=logical_date,
            conf=conf,
        )

        try:
            response = self.__api_instance.post_dag_run(dag_id, dag_run)
            return response
        except Exception as e:
            self.log.error(f"Exception when calling DAGRunApi->post_dag_run: {e}")

    def get_dag_run(self, dag_id: str, dag_run_id: str):
        try:
            response = self.__api_instance.get_dag_run(dag_id=dag_id, dag_run_id=dag_run_id)
            return response
        except NotFoundException as e:
            self.log.info(
                f"DAG is not found when calling DAGApi->get_task: {e}, dag_id: {dag_id}, dag_run_id: {dag_run_id}"
            )
            return None
        except Exception as e:
            self.log.error(f"Exception when calling DAGApi->get_task: {e}, dag_id: {dag_id}, dag_run_id: {dag_run_id}")

    def clear_dag_run(self, dag_id: str, dag_run_id: str):
        try:
            clear_dag_run = ClearDagRun(
                dry_run=False,
            )
            response = self.__api_instance.clear_dag_run(dag_id, dag_run_id, clear_dag_run)
            return response
        except Exception as e:
            self.log.error(f"Exception when calling DAGRunApi->clear_dag_run: {e}")
