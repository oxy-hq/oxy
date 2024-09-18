from uuid import uuid4

import pytest
from hypothesis import given
from onyx.catalog.adapters.task_queues import AbstractTaskQueuePublisher
from onyx.catalog.features.data_sources.commands import (
    CreateIntegration,
    IntegrationCreated,
    create_integration,
    integration_created,
)
from onyx.catalog.models.errors import IntegrationNotFound
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.adapters.secrets_manager import AbstractSecretsManager
from onyx.shared.adapters.worker import AbstractWorker
from onyx.shared.config import OnyxConfig
from onyx.shared.services.dispatcher import AbstractDispatcher
from onyx.shared.services.message_bus import EventCollector

from tests.helpers import strategies as st


@given(create_integration_command=st.create_integration_command)
@pytest.mark.asyncio(scope="session")
async def test_create_integration(
    in_memory_uow: AbstractUnitOfWork,
    fake_task_queue: AbstractTaskQueuePublisher,
    fake_secrets_manager: AbstractSecretsManager,
    create_integration_command: CreateIntegration,
    onyx_config: OnyxConfig,
    dispatcher: AbstractDispatcher,
    worker: AbstractWorker,
):
    events_collector = EventCollector()
    integration = create_integration(create_integration_command, in_memory_uow, fake_secrets_manager, events_collector)
    assert integration.id is not None
    assert integration.slug == create_integration_command.configuration.root.slug
    assert integration.name == create_integration_command.name
    assert str(integration.organization_id) == str(create_integration_command.organization_id)
    event = list(events_collector.collect())[0]
    assert isinstance(event, IntegrationCreated)
    assert str(event.organization_id) == str(create_integration_command.organization_id)
    assert str(event.integration_id) == str(integration.id)
    await integration_created(event, worker, onyx_config, in_memory_uow, fake_task_queue, dispatcher)
    assert integration.id == fake_task_queue.publish_integration_created(integration).source_id


@pytest.mark.asyncio(scope="session")
async def test_trigger_not_found_event(
    in_memory_uow: AbstractUnitOfWork,
    fake_task_queue: AbstractTaskQueuePublisher,
    onyx_config: OnyxConfig,
    dispatcher: AbstractDispatcher,
    worker: AbstractWorker,
):
    event = IntegrationCreated(organization_id=uuid4(), integration_id=uuid4())
    with pytest.raises(IntegrationNotFound):
        await integration_created(event, worker, onyx_config, in_memory_uow, fake_task_queue, dispatcher)
