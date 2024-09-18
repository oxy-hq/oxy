import uuid

import pytest
from onyx.catalog.adapters.search import AgentDocument, FakeSearchClient
from onyx.catalog.features.agent.queries import SearchAgents
from onyx.catalog.models.agent import Agent
from onyx.catalog.models.agent_version import AgentVersion
from onyx.shared.services.base import Service
from sqlalchemy.orm import Session


@pytest.fixture(scope="session")
def agent(sqlalchemy_session: Session):
    agent = Agent(
        organization_id=uuid.uuid4(),
    )
    version = AgentVersion(
        name="Test Persona",
        instructions="Test Instructions",
        description="Test Description",
        greeting="Test Greeting",
        subdomain="test-persona",
        avatar="test-avatar",
        agent_metadata={"test": "metadata"},
    )

    agent.versions.append(version)
    sqlalchemy_session.add(agent)
    sqlalchemy_session.flush()
    sqlalchemy_session.refresh(agent)

    agent.published_version = version
    sqlalchemy_session.commit()

    try:
        yield agent
    finally:
        agent_id = agent.id
        sqlalchemy_session.query(Agent).filter_by(id=agent_id).delete()
        sqlalchemy_session.commit()


@pytest.mark.asyncio(scope="session")
async def test_search_persona(service: Service, agent: Agent, fake_search_client: FakeSearchClient):
    message = SearchAgents(query=agent.published_version.name, user_email="hai@hyperquery.ai")
    await fake_search_client.index_agent(
        AgentDocument(
            id=agent.id,
            avatar=agent.published_version.avatar,
            description=agent.published_version.description,
            subdomain=agent.published_version.subdomain,
            name=agent.published_version.name,
            conversation_starters=agent.published_version.starters,
        )
    )
    result = await service.handle(message)
    agents = result["agents"]
    is_agent_view = result["is_agent_view"]
    assert len(agents) > 0
    assert str(agent.id) in [p["id"] for p in agents]
    assert is_agent_view is True
