from onyx.catalog.adapters.chat_client import AbstractChatClient
from onyx.catalog.models.agent import Agent, agent_categories_association
from onyx.catalog.models.agent_version import AgentVersion
from onyx.catalog.models.cms.agent_category import AgentCategory
from onyx.catalog.models.cms.agent_featured import AgentFeatured
from onyx.catalog.models.cms.tabs import DiscoverTab
from onyx.catalog.models.user_agent_like import UserAgentLike
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.catalog.utils.mock import get_buffer_numbers
from onyx.shared.models.base import Command, Message
from onyx.shared.models.constants import FEATURED_CATEGORY
from pydantic import BaseModel
from sqlalchemy import false, func, select
from sqlalchemy.orm import Session, selectinload


class Tab(BaseModel):
    name: str
    categories: list[str]


class PublishDiscoverTabs(Message[list[tuple[str, int]]]):
    tabs: list[Tab]


def publish_discover_tabs(request: PublishDiscoverTabs, uow: AbstractUnitOfWork):
    new_tabs: list[DiscoverTab] = []
    for idx, tab in enumerate(request.tabs):
        tab_categories = uow.cms.get_categories(tab.categories)
        new_tabs.append(DiscoverTab(name=tab.name, categories=tab_categories, position=idx + 1))
    uow.cms.update_tabs(new_tabs)
    uow.commit()
    return [(tab.name, tab.position) for tab in new_tabs]


class ImportAgentCategories(Command[dict[str, bool]]):
    items: dict[str, list[str]]


def import_agent_categories(request: ImportAgentCategories, uow: AbstractUnitOfWork):
    agents = uow.agents.list_by_subdomains(list(request.items.keys()))
    results: dict[str, bool] = {subdomain: False for subdomain in request.items.keys()}
    for agent in agents:
        subdomain = agent.published_version.subdomain
        categories = uow.cms.get_categories(request.items[agent.published_version.subdomain])
        agent.categories = categories
        results[subdomain] = True
    uow.commit()
    return results


class FeaturedAgents(Command[bool]):
    sub_domains: list[str]


def featured_agents(request: FeaturedAgents, uow: AbstractUnitOfWork):
    agents = {agent.published_version.subdomain: agent for agent in uow.agents.list_by_subdomains(request.sub_domains)}
    new_featured = []

    for idx, subdomain in enumerate(request.sub_domains):
        agent = agents.get(subdomain)
        if agent and not agent.is_deleted:
            new_featured.append(
                AgentFeatured(
                    agent_id=agent.id,
                    position=idx + 1,
                )
            )

    uow.cms.update_featured(new_featured)
    uow.commit()
    return True


class ListDiscoverTabs(Command[list[dict[str, str]]]):
    ...


def list_discover_tabs(request: ListDiscoverTabs, session: Session):
    stmt = select(DiscoverTab).order_by(DiscoverTab.position)
    tabs = session.scalars(stmt).all()
    return [
        {
            "label": tab.name,
            "value": tab.name,
        }
        for tab in [DiscoverTab.featured(), *tabs]
    ]


class ListAgentsByTab(Command[list[AgentFeatured]]):
    name: str


async def list_agents_by_tab(request: ListAgentsByTab, session: Session, chat_client: AbstractChatClient):
    stmt = (
        select(Agent)
        .where(Agent.published_version_id.is_not(None), Agent.is_deleted == false())
        .options(selectinload(Agent.published_version))
    )
    if request.name == FEATURED_CATEGORY:
        stmt = stmt.join(AgentFeatured, AgentFeatured.agent_id == Agent.id).order_by(AgentFeatured.position)
    else:
        tab = session.execute(select(DiscoverTab).where(DiscoverTab.name == request.name)).scalar()
        if not tab:
            return []

        stmt = (
            stmt.join(agent_categories_association, agent_categories_association.c.agent_id == Agent.id)
            .join(AgentCategory, agent_categories_association.c.category_id == AgentCategory.id)
            .where(AgentCategory.id.in_([category.id for category in tab.categories]))
        )

    result = session.scalars(stmt).all()
    messages_count = await chat_client.count_messages([row.id for row in result])
    likes_count_stmt = (
        select(UserAgentLike.agent_id, func.count(UserAgentLike.agent_id).label("total_likes"))
        .group_by(UserAgentLike.agent_id)
        .where(UserAgentLike.agent_id.in_([row.id for row in result]))
    )
    likes_count = {row.agent_id: row.total_likes for row in session.execute(likes_count_stmt).all()}
    agents = []
    for agent in result:
        likes_buffer, messages_buffer = get_buffer_numbers(agent.id, agent.organization_id)
        agent_dict = agent.to_published_dict()
        assert agent_dict
        agent_dict["total_likes"] = likes_count.get(agent.id, 0) + likes_buffer
        agent_dict["total_messages"] = messages_count.get(agent.id, 0) + messages_buffer
        agents.append(agent_dict)

    return agents


class ListAgentsByOrganization(Command[list[AgentFeatured]]):
    organization_id: str


async def list_agents_by_organization(
    request: ListAgentsByOrganization, session: Session, chat_client: AbstractChatClient
):
    stmt = (
        select(Agent)
        .where(
            Agent.published_version_id.is_not(None),
            Agent.is_deleted == false(),
            Agent.organization_id == request.organization_id,
        )
        .options(selectinload(Agent.published_version))
    )

    result = session.scalars(stmt).all()
    messages_count = await chat_client.count_messages([row.id for row in result])

    agents = []
    for agent in result:
        _, messages_buffer = get_buffer_numbers(agent.id, agent.organization_id)
        agent_dict = agent.to_published_dict()
        assert agent_dict
        agent_dict["total_likes"] = 0
        agent_dict["total_messages"] = messages_count.get(agent.id, 0) + messages_buffer
        agents.append(agent_dict)

    return agents


class LookupAgentSubdomain(Command[str]):
    name: str


def lookup_agent_subdomain(request: LookupAgentSubdomain, session: Session):
    terms = "%".join(request.name.split())
    stmt = (
        select(Agent)
        .join(AgentVersion, Agent.published_version_id == AgentVersion.id)
        .where(AgentVersion.name.ilike(terms))
    )
    found = session.scalars(stmt).all()
    return found[0].published_version.subdomain if found else None
