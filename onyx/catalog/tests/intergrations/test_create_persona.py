import asyncio
from uuid import uuid4

import pytest
from onyx.catalog.adapters.search import FakeSearchClient
from onyx.catalog.features.agent import CreateAgent
from onyx.catalog.features.agent.commands import DeleteAgent, PublishAgent
from onyx.shared.services.base import Service


@pytest.mark.asyncio(scope="session")
async def test_index_agent(service: Service, fake_search_client: FakeSearchClient):
    message = CreateAgent(
        organization_id=uuid4(),
        name="Test Persona",
        instructions="Test Instructions",
        description="Test Description",
        greeting="Test Greeting",
        subdomain="test-persona",
        avatar="test-avatar",
        agent_metadata={"test": "metadata"},
    )
    agent = await service.handle(message)
    assert agent.id is not None
    assert agent.organization_id == message.organization_id
    await service.handle(
        PublishAgent(
            id=agent.id,
        )
    )
    await asyncio.sleep(0.01)
    agent_doc = fake_search_client.agents.get(str(agent.id))
    assert agent_doc is not None
    assert agent_doc.name == message.name
    await service.handle(
        DeleteAgent(
            id=agent.id,
        )
    )
    await asyncio.sleep(0.01)
    agent_doc = fake_search_client.agents.get(str(agent.id))
    assert agent_doc is None
