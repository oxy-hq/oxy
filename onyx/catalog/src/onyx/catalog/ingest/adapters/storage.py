from datetime import datetime, timedelta

from onyx.catalog.ingest.base.storage import Storage
from onyx.catalog.ingest.base.types import Identity, Interval
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.models.constants import ConnectionSyncStatus


class IntegrationStateStorage(Storage):
    def __init__(self, uow: AbstractUnitOfWork) -> None:
        self.uow = uow

    async def read_stream_state(self, identity, stream_name) -> list[Interval]:
        ingest_state = self.uow.integrations.get_or_create_ingest_state(identity.datasource_id)  # type: ignore
        return [Interval(start=start, end=end) for start, end in ingest_state.bookmarks.get(stream_name, [])]

    async def write_stream_state(self, identity, stream_name, intervals):
        ingest_state = self.uow.integrations.get_ingest_state_for_update(identity.datasource_id)  # type: ignore
        state = [(interval.start, interval.end) for interval in intervals]
        ingest_state.bookmarks[stream_name] = state
        self.uow.commit()

    async def write_source_state(
        self, identity: Identity, last_success_bookmark: datetime | None = None, error: str | None = None
    ):
        ingest_state = self.uow.integrations.get_ingest_state_for_update(identity.datasource_id)  # type: ignore
        if error is None:
            sync_status = ConnectionSyncStatus.success
            ingest_state.last_success_bookmark = last_success_bookmark
        else:
            sync_status = ConnectionSyncStatus.error
        ingest_state.sync_error = error
        ingest_state.sync_status = sync_status
        ingest_state.last_synced_at = datetime.now()
        self.uow.commit()

    async def generate_request_interval(self, identity: Identity, default_beginning_delta: timedelta) -> Interval:
        ingest_state = self.uow.integrations.get_or_create_ingest_state(identity.datasource_id)  # type: ignore

        # Reset the sync status to syncing
        ingest_state.sync_status = ConnectionSyncStatus.syncing
        ingest_state.sync_error = None
        self.uow.commit()

        last_success_bookmark = ingest_state.last_success_bookmark
        end_ts = datetime.now()
        start_ts = datetime.now() - default_beginning_delta
        if last_success_bookmark is not None:
            start_ts = last_success_bookmark
        start_ts = int(start_ts.timestamp())
        end_ts = int(end_ts.timestamp())
        return Interval(start=int(start_ts), end=end_ts)
