from datetime import datetime
from typing import Iterable, NamedTuple, Sequence, TypedDict
from uuid import UUID

from onyx.catalog.adapters.gmail import AbstractGmail, GmailUserInfo
from onyx.catalog.adapters.notion import AbstractNotion, NotionUserInfo
from onyx.catalog.adapters.salesforce import AbstractSalesforce, SalesforceUserInfo
from onyx.catalog.adapters.slack import AbstractSlack, SlackUserInfo
from onyx.catalog.adapters.task_queues import AbstractTaskQueuePublisher
from onyx.catalog.models.connection import Connection
from onyx.catalog.models.ingest_state import IngestState
from onyx.catalog.models.integration import Integration
from onyx.catalog.models.namespace import Namespace
from onyx.catalog.models.task import Task
from onyx.shared.config import OnyxConfig
from onyx.shared.models.base import Message
from onyx.shared.models.constants import ConnectionSlugChoices, IntegrationSlugChoices, map_legacy_state
from sqlalchemy import and_, func, null, or_, select, union_all
from sqlalchemy.orm import Session


class GetSalesforceRefreshToken(Message[tuple[str, str]]):
    code: str


class GetSalesforceUserInfo(Message[SalesforceUserInfo]):
    token: str


def get_salesforce_refresh_token(request: GetSalesforceRefreshToken, salesforce: AbstractSalesforce) -> str:
    return salesforce.get_refresh_token(code=request.code)


def get_salesforce_user_info(request: GetSalesforceUserInfo, salesforce: AbstractSalesforce):
    return salesforce.get_user_info(token=request.token)


class GetSlackTokenAndInfo(Message[SlackUserInfo]):
    code: str


def get_slack_oauth_access(request: GetSlackTokenAndInfo, slack: AbstractSlack):
    return slack.get_oauth_access(code=request.code)


class GetGmailRefreshToken(Message[tuple[str, str]]):
    code: str


class GetGmailUserInfo(Message[GmailUserInfo]):
    token: str


def get_gmail_refresh_token(request: GetGmailRefreshToken, gmail: AbstractGmail):
    return gmail.get_refresh_token(code=request.code)


def get_gmail_user_info(request: GetGmailUserInfo, gmail: AbstractGmail):
    return gmail.get_user_info(token=request.token)


class GetNotionRefreshToken(Message[str]):
    code: str


class GetNotionUserInfo(Message[NotionUserInfo]):
    token: str


def get_notion_access_token(request: GetNotionRefreshToken, notion: AbstractNotion):
    return notion.get_access_token(code=request.code)


def get_notion_user_info(request: GetNotionUserInfo, notion: AbstractNotion):
    return notion.get_user_info(request.token)


class DataSourceRow(NamedTuple):
    id: UUID
    name: str
    slug: str
    task_id: str | None
    external_id: str | None
    queue_system: str | None
    last_run_at: str | None
    integration_metadata: dict[str, str] | None
    namespace: str
    sync_status: str | None
    last_synced_at: datetime | None


class DataSource(TypedDict):
    id: str
    name: str
    slug: str
    task_id: str | None
    namespace: str
    integration_metadata: dict[str, str]
    status: str | None
    date_done: str | None


class ListDataSource(Message[Iterable[DataSource]]):
    organization_id: UUID
    user_id: UUID
    with_status: bool = True


def list_data_sources(
    request: ListDataSource,
    session: Session,
    task_queue: AbstractTaskQueuePublisher,
    config: OnyxConfig,
) -> Iterable[DataSource]:
    data_sources_stmt = __build_list_data_sources_query(request=request)
    data_sources: Sequence[DataSourceRow] = [
        DataSourceRow(*row.tuple()) for row in session.execute(data_sources_stmt).all()
    ]
    return __data_sources_with_status(data_sources, task_queue, config, request.with_status)


class GetIntegrationByName(Message[Integration]):
    name: str
    organization_id: UUID


def get_integration_by_name(request: GetIntegrationByName, session: Session):
    stmt = select(Integration).where(
        Integration.organization_id == request.organization_id,
        Integration.name == request.name,
    )
    found = session.scalars(stmt).one_or_none()
    return found


class GetConnectionByName(Message[Integration]):
    name: str
    organization_id: UUID


def get_connection_by_name(request: GetConnectionByName, session: Session):
    stmt = select(Connection).where(
        Connection.organization_id == request.organization_id,
        Connection.name == request.name,
    )
    found = session.scalars(stmt).one_or_none()
    return found


def list_integrations(
    ids: list[UUID],
    session: Session,
    task_queue: AbstractTaskQueuePublisher,
    config: OnyxConfig,
):
    integrations_stmt = __build_list_data_sources_by_integration_ids_query(ids)
    data_sources: Sequence[DataSourceRow] = [
        DataSourceRow(*row.tuple()) for row in session.execute(integrations_stmt).all()
    ]
    return __data_sources_with_status(data_sources, task_queue, config)


def __build_latest_task_subquery():
    return (
        select(
            Task.source_id,
            func.max(Task.created_at).label("latest_created_at"),
        )
        .group_by(Task.source_id)
        .subquery()
    )


def __build_select_integration_query():
    latest_task_subquery = __build_latest_task_subquery()
    return (
        select(
            Integration.id.label("id"),
            Integration.name.label("name"),
            Integration.slug.label("slug"),
            Task.id.label("task_id"),
            Task.external_id.label("external_id"),
            Task.queue_system.label("queue_system"),
            Task.created_at.label("last_run_at"),
            Integration.integration_metadata.label("integration_metadata"),
            Namespace.name.label("namespace"),
            IngestState.sync_status.label("sync_status"),
            IngestState.last_synced_at.label("last_synced_at"),
        )
        .select_from(Integration)
        .join(Namespace, Integration.namespace_id == Namespace.id)
        .join(latest_task_subquery, latest_task_subquery.c.source_id == Integration.id, isouter=True)
        .join(IngestState, IngestState.integration_id == Integration.id, isouter=True)
        .join(
            Task,
            and_(Task.source_id == Integration.id, Task.created_at == latest_task_subquery.c.latest_created_at),
            isouter=True,
        )
    )


def __build_select_connection_query():
    latest_task_subquery = __build_latest_task_subquery()
    return (
        select(
            Connection.id.label("id"),
            Connection.name.label("name"),
            Connection.slug.label("slug"),
            Task.id.label("task_id"),
            Task.external_id.label("external_id"),
            Task.queue_system.label("queue_system"),
            Task.created_at.label("last_run_at"),
            null().label("integration_metadata"),
            Namespace.name.label("namespace"),
            Connection.sync_status.label("sync_status"),
            Connection.updated_at.label("last_synced_at"),
        )
        .select_from(Connection)
        .join(Namespace, Connection.namespace_id == Namespace.id)
        .join(latest_task_subquery, latest_task_subquery.c.source_id == Connection.id, isouter=True)
        .join(
            Task,
            and_(Task.source_id == Connection.id, Task.created_at == latest_task_subquery.c.latest_created_at),
            isouter=True,
        )
    )


def __build_list_data_sources_query(request: ListDataSource):
    namespaces_condition = and_(
        Namespace.organization_id == request.organization_id,
        or_(Namespace.owner_id == request.user_id, Namespace.owner_id == request.organization_id),
    )
    integrations_stmt = __build_select_integration_query().where(namespaces_condition)
    connections_stmt = __build_select_connection_query().where(namespaces_condition)
    stmt = union_all(integrations_stmt, connections_stmt).order_by("last_run_at")
    return stmt


def __build_list_data_sources_by_integration_ids_query(ids: list[UUID]):
    integration_stmt = __build_select_integration_query().where(
        Integration.id.in_(ids),
    )
    return integration_stmt


def __data_sources_with_status(
    data_sources: Sequence[DataSourceRow],
    task_queue: AbstractTaskQueuePublisher,
    config: OnyxConfig,
    with_status: bool = True,
) -> list[DataSource]:
    records: list[DataSource] = []
    for data_source in data_sources:
        task_result = None
        is_temporal_enabled = config.temporal.enabled and data_source.slug in [
            IntegrationSlugChoices.gmail,
            *ConnectionSlugChoices,
        ]
        if data_source.external_id and data_source.queue_system and with_status and not is_temporal_enabled:
            task_result = task_queue.get_task_result_by_id(data_source.external_id, data_source.slug)

        record: DataSource = {
            "id": data_source.id,  # type: ignore
            "name": data_source.name,
            "slug": data_source.slug,
            "task_id": data_source.task_id,
            "namespace": data_source.namespace,
            "integration_metadata": data_source.integration_metadata or {},
            "status": None,
            "date_done": None,
        }
        if task_result:
            record["status"] = task_result.state
            record["date_done"] = task_result.date_done

        if is_temporal_enabled and with_status:
            record["status"] = map_legacy_state(data_source.sync_status)
            record["date_done"] = data_source.last_synced_at

        records.append(record)
    return records
