from typing import cast
from uuid import UUID

from onyx.catalog.adapters.search import AbstractSearchClient, AgentDocument
from onyx.catalog.models.agent import Agent
from onyx.catalog.models.agent_version import AgentVersion
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.models.base import Command, Event
from onyx.shared.services.message_bus import EventCollector


class CreateAgent(Command[Agent]):
    name: str
    instructions: str
    description: str
    organization_id: UUID
    avatar: str
    greeting: str
    subdomain: str
    agent_metadata: dict


def create_agent(request: CreateAgent, uow: AbstractUnitOfWork, event_collector: EventCollector):
    agent = Agent(
        organization_id=request.organization_id,
    )
    dev_version = AgentVersion(
        name=request.name,
        instructions=request.instructions,
        description=request.description,
        greeting=request.greeting,
        subdomain=request.subdomain,
        avatar=request.avatar,
        agent_metadata=request.agent_metadata,
    )
    agent.versions.append(dev_version)
    new_agent = uow.agents.add(agent)
    agent.dev_version = dev_version
    uow.commit()

    if new_agent.id:
        event_collector.publish(
            AgentCreated(
                agent_id=new_agent.id,
                organization_id=new_agent.organization_id,
            )
        )

    return new_agent


class AgentCreated(Event):
    agent_id: UUID
    organization_id: UUID


def agent_created(event: AgentCreated):
    # TODO: handle this method
    return


class UpdateAgentInfo(Command[dict]):
    id: UUID
    name: str
    instructions: str
    description: str
    avatar: str
    greeting: str
    subdomain: str


def update_agent_info(request: UpdateAgentInfo, uow: AbstractUnitOfWork):
    agent: Agent = uow.agents.get_by_id(request.id)
    dev_version = agent.dev_version
    if not agent.have_dev_version:
        dev_version = __create_dev_version(agent, uow)

    dev_version.subdomain = request.subdomain
    dev_version.name = request.name
    dev_version.instructions = request.instructions
    dev_version.description = request.description
    dev_version.greeting = request.greeting

    if request.avatar != "":
        dev_version.avatar = request.avatar
    uow.agent_versions.add(dev_version)
    agent.dev_version_id = dev_version.id
    uow.commit()
    return agent.to_dev_dict()


class UpdateAgentKnowledge(Command[dict]):
    id: UUID
    integrations_ids: list[UUID]
    avatar: str
    knowledge: str
    starters: list[str]


def update_agent_knowledge(request: UpdateAgentKnowledge, uow: AbstractUnitOfWork):
    agent: Agent = uow.agents.get_by_id(request.id)
    dev_version: AgentVersion = agent.dev_version
    if not agent.have_dev_version:
        dev_version = __create_dev_version(agent, uow)
    dev_version.knowledge = request.knowledge

    if len(request.starters) > 0:
        dev_version.starters = request.starters

    if request.avatar != "":
        dev_version.avatar = request.avatar
    integrations = uow.integrations.list_by_ids(request.integrations_ids)
    connections = uow.connections.list_by_ids(request.integrations_ids)
    if len(integrations) + len(connections) != len(request.integrations_ids):
        raise ValueError("Some integrations not found")
    dev_version.integrations = integrations
    dev_version.connections = connections
    agent.dev_version_id = dev_version.id
    uow.commit()
    return agent.to_dev_dict()


class DiscardAgentChanges(Command[dict]):
    id: UUID


def discard_agent_changes(request: DiscardAgentChanges, uow: AbstractUnitOfWork):
    agent: Agent = uow.agents.get_by_id(request.id)
    dev_version_id = agent.dev_version_id
    __create_dev_version(agent, uow)
    uow.agent_versions.delete(dev_version_id)
    uow.commit()
    return agent.to_dev_dict()


class DeleteAgent(Command[bool]):
    id: UUID


def delete_agent(request: DeleteAgent, uow: AbstractUnitOfWork, collector: EventCollector):
    agent = uow.agents.get_by_id(request.id)

    if not agent:
        return False

    agent.is_deleted = True
    uow.commit()
    collector.publish(AgentDeleted(agent_id=agent.id))
    return True


class AgentDeleted(Event):
    agent_id: UUID


async def agent_deleted(event: AgentDeleted, search_client: AbstractSearchClient):
    await search_client.delete_agent(agent_id=event.agent_id)


class CreateDevAgent(Command[dict]):
    id: UUID


def create_dev_agent(request: CreateDevAgent, uow: AbstractUnitOfWork):
    """
    if there is a published version, clone it and set the dev version
    else return None
    """
    agent = uow.agents.get_by_id(request.id)
    if not agent:
        raise ValueError("Agent not found")
    __create_dev_version(agent, uow)
    uow.commit()
    return agent.to_dev_dict()


class PublishAgent(Command[dict]):
    id: UUID


def publish_agent(request: PublishAgent, uow: AbstractUnitOfWork, collector: EventCollector):
    agent = uow.agents.get_by_id(request.id)
    if not agent:
        raise ValueError("Agent not found")

    agent.published_version_id = agent.dev_version_id
    agent.published_version = agent.dev_version
    __create_dev_version(agent, uow)
    uow.commit()

    if agent.id:
        version = cast(AgentVersion, agent.published_version)
        collector.publish(
            AgentPublished(
                agent=AgentDocument(
                    id=agent.id,
                    name=version.name,
                    description=version.description,
                    conversation_starters=version.starters,
                    avatar=version.avatar,
                    subdomain=version.subdomain,
                )
            )
        )

    return agent.to_published_dict()


class AgentPublished(Event):
    agent: AgentDocument


async def agent_published(event: AgentPublished, search_client: AbstractSearchClient):
    await search_client.index_agent(agent=event.agent)


# TODO: check usage of this handlers
class PublicAgent(Command[Agent]):
    agent_id: UUID


def public_agent(request: PublicAgent, uow: AbstractUnitOfWork):
    agent: Agent | None = uow.agents.get_by_id(request.agent_id)
    if agent is None:
        raise ValueError("Agent not found")

    if agent.dev_version is None:
        raise ValueError("Agent dev version not found")

    agent.published_version = agent.dev_version
    agent.dev_version.is_published = True
    uow.commit()
    return agent


def __create_dev_version(agent: Agent, uow: AbstractUnitOfWork):
    if agent.published_version:
        dev_version = agent.published_version.clone()
        # track the cloned prompts
        for prompt in dev_version.prompts:
            uow.prompts.add(prompt)
        dev_version.is_published = False
        uow.agent_versions.add(dev_version)
        agent.dev_version = dev_version
        return dev_version
    return None
