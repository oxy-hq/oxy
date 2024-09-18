import random
from typing import cast as CAST
from uuid import UUID

from onyx.catalog.adapters.chat_client import AbstractChatClient
from onyx.catalog.adapters.search import AbstractSearchClient
from onyx.catalog.adapters.task_queues import AbstractTaskQueuePublisher
from onyx.catalog.features.data_sources import list_integrations
from onyx.catalog.models.agent import Agent, SearchAgent, agent_categories_association
from onyx.catalog.models.agent_version import AgentVersion
from onyx.catalog.models.cms.agent_category import FEATURED_CATEGORY, AgentCategory
from onyx.catalog.models.user_agent_like import UserAgentLike
from onyx.catalog.utils.mock import get_buffer_numbers
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Message
from onyx.shared.models.common import AgentDisplayInfo
from onyx.shared.models.common import AgentInfo as InternalAgentInfo
from pydantic import BaseModel
from sqlalchemy import String, and_, asc, cast, desc, false, func, or_, select, true
from sqlalchemy.orm import Session, aliased, selectinload
from typing_extensions import TypedDict

logger = get_logger(__name__)


class AgentVersionInfo(TypedDict):
    id: str
    name: str
    instructions: str
    description: str
    avatar: str
    greeting: str
    subdomain: str
    knowledge: str
    starters: list[str]
    integrations: list[dict]
    agent_metadata: dict[str, str]
    is_published: bool
    is_changed: bool


class AgentInfo(TypedDict):
    id: str
    organization_id: str
    published_version: AgentVersionInfo | None
    dev_version: AgentVersionInfo | None


class ListAgents(Message[list[AgentInfo]]):
    organization_id: UUID


def list_agents(
    request: ListAgents,
    session: Session,
) -> list[AgentInfo]:
    stmt = (
        select(Agent)
        .where(Agent.organization_id == request.organization_id)
        .where(Agent.is_deleted == false())
        .order_by(desc(Agent.created_at))
        .options(selectinload(Agent.published_version).selectinload(AgentVersion.integrations))
    )
    agents = session.scalars(stmt).all()
    logger.debug(f"agents: {agents}")
    return [CAST(AgentInfo, agent.to_dict()) for agent in agents]


class ListRecentAgents(Message[list[AgentInfo]]):
    organization_id: UUID


def list_recent_agents(
    request: ListRecentAgents,
    session: Session,
) -> list[AgentInfo]:
    DevVersion = aliased(Agent.dev_version.mapper.class_)

    stmt = (
        select(Agent)
        .join(DevVersion, Agent.dev_version)
        .where(Agent.organization_id == request.organization_id)
        .where(Agent.is_deleted == false())
        .options(selectinload(Agent.published_version), selectinload(Agent.dev_version))
        .order_by(desc(DevVersion.updated_at))
        .limit(5)
    )
    agents = session.scalars(stmt).all()
    logger.debug(f"agents: {agents}")
    return [CAST(AgentInfo, agent.to_dict()) for agent in agents]


class GetPublishedAgentVersionByAgentIdsResult(TypedDict):
    published_version: AgentVersion | None
    is_deleted: bool


class GetPublishedAgentVersionByAgentIds(Message[list[GetPublishedAgentVersionByAgentIdsResult | None]]):
    ids: list[UUID]
    include_integrations: bool = False


def get_published_agent_version_by_agent_ids(request: GetPublishedAgentVersionByAgentIds, session: Session):
    query_options = selectinload(Agent.published_version)
    stmt = select(Agent).where(Agent.id.in_(request.ids)).options(query_options)
    agents = session.scalars(stmt).all()
    return [{"published_version": agent.published_version, "is_deleted": agent.is_deleted} for agent in agents]


class GetAgent(Message[dict]):
    agent_id: UUID


def get_agent(
    request: GetAgent,
    session: Session,
    task_queue: AbstractTaskQueuePublisher,
    config: OnyxConfig,
):
    stmt = select(Agent).where(Agent.id == request.agent_id)
    agent = session.scalars(stmt).one_or_none()
    if agent is None:
        raise ValueError("Agent not found")
    rs = agent.to_dict()
    rs["dev_version"]["integrations"] = list_integrations(
        [integration.id for integration in agent.dev_version.integrations],
        session,
        task_queue,
        config,
    )
    return rs


class GetPublishedAgent(Message[Agent]):
    agent_id: UUID


def get_published_agent(
    request: GetPublishedAgent,
    session: Session,
):
    stmt = (
        select(Agent)
        .where(Agent.id == request.agent_id)
        .options(selectinload(Agent.published_version).selectinload(AgentVersion.integrations))
    )
    agent = session.scalars(stmt).one_or_none()
    return agent


class Pagination(BaseModel):
    page: int
    page_size: int


class ListPublishedAgents(Message[tuple[list[dict], int]]):
    pagination: Pagination


def list_published_agents(request: ListPublishedAgents, session: Session):
    stmt = (
        select(Agent)
        .where(Agent.published_version_id.is_not(None))
        .where(Agent.is_deleted == false())
        .order_by(desc(Agent.created_at))
        .offset((request.pagination.page - 1) * request.pagination.page_size)
        .limit(request.pagination.page_size)
        .options(selectinload(Agent.published_version))
    )
    agents = session.scalars(stmt).all()
    count_stmt = select(func.count(Agent.id)).where(Agent.published_version is not None)
    count = session.scalars(count_stmt).one()

    return [agent.to_published_dict() for agent in agents], count


class ListAgentCategories(Message[list[AgentCategory]]):
    limit: int = 6


def list_agent_categories(request: ListAgentCategories, session: Session):
    stmt = (
        select(AgentCategory.value, func.count(Agent.id).label("agents_count"))
        .outerjoin(agent_categories_association, agent_categories_association.c.category_id == AgentCategory.id)
        .outerjoin(Agent, agent_categories_association.c.agent_id == Agent.id)
        .group_by(AgentCategory.id)
        .order_by(desc(func.count(Agent.id)))
        .limit(request.limit)
    )
    categories = session.execute(stmt).all()
    return [(row.value, row.agents_count) for row in categories]


class ListPublishedAgentsByCategory(Message[tuple[list[dict]]]):
    category: str


async def list_published_agents_by_category(
    request: ListPublishedAgentsByCategory, session: Session, chat_client: AbstractChatClient
):
    if request.category == FEATURED_CATEGORY:
        category_filter = Agent.is_featured == true()
    else:
        category_filter = AgentCategory.value == request.category

    stmt = (
        select(
            Agent,
            func.count(UserAgentLike.id).label("total_likes"),
        )
        .outerjoin(UserAgentLike, Agent.id == UserAgentLike.agent_id)
        .join(agent_categories_association, agent_categories_association.c.agent_id == Agent.id)
        .join(AgentCategory, agent_categories_association.c.category_id == AgentCategory.id)
        .where(Agent.published_version_id.is_not(None), Agent.is_deleted == false(), category_filter)
        .group_by(Agent.id)
        .order_by(asc(Agent.weight))
        .options(selectinload(Agent.published_version))
    )
    result = session.execute(stmt).all()
    messages_count = await chat_client.count_messages([row.Agent.id for row in result])

    agents = []
    for row in result:
        agent = row.Agent
        total_likes = row.total_likes

        if agent:
            likes_buffer, messages_buffer = get_buffer_numbers(agent.id, agent.organization_id)
            agent_dict = agent.to_published_dict()
            agent_dict["total_likes"] = total_likes + likes_buffer
            agent_dict["total_messages"] = messages_count.get(agent.id, 0) + messages_buffer
            agents.append(agent_dict)

    return agents


class SearchAgentsResult(TypedDict):
    agents: list[dict]
    is_agent_view: bool


class SearchAgents(Message[SearchAgentsResult]):
    query: str
    user_email: str


async def search_agents(
    request: SearchAgents,
    search_client: AbstractSearchClient,
    chat_client: AbstractChatClient,
    session: Session,
):
    is_agent_view = search_client.is_search_agent(request.query)
    agents = await search_client.search_agents(request.query)
    agent_ids = [agent.id for agent in agents]
    stmt = (
        select(
            Agent.id,
            func.count(UserAgentLike.id).label("total_likes"),
        )
        .join(UserAgentLike, Agent.id == UserAgentLike.agent_id)
        .where(Agent.id.in_(agent_ids))
        .group_by(Agent.id)
    )
    likes_count = dict([row.tuple() for row in session.execute(stmt).all()])
    messages_count = await chat_client.count_messages(agent_ids)
    logger.info(f"search_agents: messages_count: {messages_count} likes_count: {likes_count}")

    result = []
    agents_with_org = session.execute(select(Agent.organization_id, Agent.id).where(Agent.id.in_(agent_ids))).all()
    agent_org_dict = {agent.id: agent.organization_id for agent in agents_with_org}
    for agent in agents:
        likes_buffer, messages_buffer = get_buffer_numbers(agent.id, agent_org_dict[agent.id])
        agent_dict = SearchAgent(
            id=agent.id,
            name=agent.name,
            description=agent.description,
            avatar=agent.avatar,
            subdomain=agent.subdomain,
            reason=agent.reason,
            total_likes=likes_count.get(agent.id, 0) + likes_buffer,
            total_messages=messages_count.get(agent.id, 0) + messages_buffer,
        ).to_dict()
        result.append(agent_dict)

    return {
        "agents": result,
        "is_agent_view": is_agent_view,
    }


class SearchViewResult(TypedDict):
    is_agent_view: bool


class SearchView(Message[SearchViewResult]):
    query: str


def search_view(
    request: SearchView,
    search_client: AbstractSearchClient,
):
    is_agent_view = search_client.is_search_agent(request.query)

    return {"is_agent_view": is_agent_view}


class SearchSuggestions(Message[list[str]]):
    limit: int


def search_suggestions(request: SearchSuggestions, session: Session):
    stmt = (
        select(AgentVersion.starters)
        .select_from(Agent)
        .join(AgentVersion, Agent.published_version_id == AgentVersion.id)
        .where(Agent.is_deleted == false(), func.cardinality(AgentVersion.starters) == 3)
        .order_by(func.random())
        .limit(request.limit)
    )
    nested_starters = session.scalars(stmt).all()
    starters: list[str] = []
    for starter in nested_starters:
        starters.extend(starter)
    starters = list(set(starters))
    random.shuffle(starters)
    return starters[:3]


class GetPublishedAgentBySubdomain(Message[dict | None]):
    subdomain: str


def get_published_agent_by_subdomain(request: GetPublishedAgentBySubdomain, session: Session):
    agent = session.scalars(__build_get_agent_by_subdomain_query(request.subdomain)).first()
    return agent.to_published_dict() if agent else None


class GetAgentIdByPublishedSubdomain(Message[UUID | None]):
    subdomain: str


def get_agent_id_by_published_subdomain(request: GetAgentIdByPublishedSubdomain, session: Session):
    stmt = (
        select(AgentVersion)
        .join(Agent, AgentVersion.agent_id == Agent.id)
        .where(
            and_(
                AgentVersion.subdomain == request.subdomain,
                Agent.published_version_id == AgentVersion.id,
            )
        )
    )
    version = session.scalars(stmt).one_or_none()
    return version.agent_id if version else None


class GetAgentVersionByName(Message[AgentVersion | None]):
    name: str


def get_agent_version_by_name(request: GetAgentVersionByName, session: Session):
    found = session.scalars(__build_get_agent_version_by_name_query(request.name)).one_or_none()
    return found


class GetAgentBySubdomain(Message[dict | None]):
    subdomain: str


def get_agent_by_subdomain(request: GetAgentBySubdomain, session: Session):
    stmt = select(Agent).where(
        or_(
            Agent.dev_version.has(AgentVersion.subdomain == request.subdomain),
            Agent.published_version.has(AgentVersion.subdomain == request.subdomain),
        ),
        Agent.is_deleted == false(),
    )
    found = session.scalars(stmt).one_or_none()
    return found.to_dict() if found else None


class GetDevAgent(Message[dict | None]):
    agent_id: UUID


def get_dev_agent(
    request: GetDevAgent,
    session: Session,
    task_queue: AbstractTaskQueuePublisher,
    config: OnyxConfig,
):
    stmt = select(Agent).where(Agent.id == request.agent_id)
    agent = session.scalars(stmt).one_or_none()
    if not agent or not agent.dev_version or agent.dev_version.is_published:
        return None
    rs = agent.to_dict()
    rs["dev_version"]["integrations"] = list_integrations(
        [integration.id for integration in agent.dev_version.integrations], session, task_queue, config
    )
    if agent.published_version:
        rs["published_version"]["integrations"] = list_integrations(
            [integration.id for integration in agent.published_version.integrations], session, task_queue, config
        )
    return rs


class IsSubdomainAvailable(Message[bool]):
    subdomain: str
    agent_id: str


def is_subdomain_available(request: IsSubdomainAvailable, session: Session):
    version = session.scalars(__build_get_agent_version_by_subdomain_query(request.subdomain)).one_or_none()

    if not version:
        return True
    if id and str(version.agent_id) == str(request.agent_id):
        return True
    return False


class IsNameAvailable(Message[bool]):
    name: str
    agent_id: str


def is_name_available(request: IsNameAvailable, session: Session):
    version = session.scalars(__build_get_agent_version_by_name_query(request.name)).first()

    if version:
        if id and str(version.agent_id) == str(request.agent_id):
            return True
        else:
            return False
    return True


class ListSubdomains(Message[dict[str, str]]):
    names: list[str]


def list_subdomains(request: ListSubdomains, session: Session):
    stmt = (
        select(AgentVersion.name, AgentVersion.subdomain)
        .join(Agent, AgentVersion.id == Agent.published_version_id)
        .where(AgentVersion.name.in_(request.names))
    )
    result = {name: None for name in request.names}
    for (
        name,
        subdomain,
    ) in session.execute(stmt).all():
        result[name] = subdomain
    return result


class ListDevAgents(Message[list[dict]]):
    organization_id: UUID


def list_dev_agents(request: ListDevAgents, session: Session):
    stmt = select(Agent).where(and_(Agent.organization_id == request.organization_id, Agent.is_deleted == false()))
    agents = session.scalars(stmt).all()
    return [agent.to_dev_dict() for agent in agents]


class GetAgentInfo(Message[InternalAgentInfo]):
    agent_id: UUID
    published: bool


def get_agent_info(request: GetAgentInfo, session: Session):
    stmt = select(Agent).where(Agent.id == request.agent_id)
    agent = session.scalars(stmt).one_or_none()
    if not agent:
        return None
    return agent.to_info(published=request.published)


class SearchAgentsByKeyword(Message[tuple[list[dict], int]]):
    keyword: str
    pagination: Pagination


def search_agents_by_keyword(request: SearchAgentsByKeyword, session: Session):
    keyword_pattern = f"%{request.keyword}%"

    stmt = (
        select(Agent)
        .join(AgentVersion, Agent.published_version_id == AgentVersion.id)
        .where(Agent.published_version_id.is_not(None))
        .where(Agent.is_deleted == false())
        .where(
            or_(
                AgentVersion.name.like(keyword_pattern),
                AgentVersion.description.like(keyword_pattern),
                AgentVersion.instructions.like(keyword_pattern),
                AgentVersion.greeting.like(keyword_pattern),
                AgentVersion.knowledge.like(keyword_pattern),
                cast(AgentVersion.starters, String).like(keyword_pattern),
                cast(AgentVersion.agent_metadata, String).like(keyword_pattern),
            )
        )
        .order_by(desc(Agent.created_at))
        .offset((request.pagination.page - 1) * request.pagination.page_size)
        .limit(request.pagination.page_size)
    )
    agents = session.execute(stmt).scalars().all()

    count_stmt = (
        select(func.count(Agent.id))
        .join(AgentVersion, Agent.published_version_id == AgentVersion.id)
        .where(Agent.published_version_id.is_not(None))
        .where(Agent.is_deleted == false())
        .where(
            or_(
                AgentVersion.name.like(keyword_pattern),
                AgentVersion.description.like(keyword_pattern),
                AgentVersion.instructions.like(keyword_pattern),
                AgentVersion.greeting.like(keyword_pattern),
                AgentVersion.knowledge.like(keyword_pattern),
                cast(AgentVersion.starters, String).like(keyword_pattern),
                cast(AgentVersion.agent_metadata, String).like(keyword_pattern),
            )
        )
    )
    count = session.execute(count_stmt).scalar()

    return [agent.to_published_dict() for agent in agents], count


def __build_get_agent_by_subdomain_query(subdomain: str):
    return select(Agent).where(
        or_(
            Agent.dev_version.has(AgentVersion.subdomain == subdomain),
            Agent.published_version.has(AgentVersion.subdomain == subdomain),
        )
    )


def __build_get_agent_version_by_subdomain_query(subdomain: str):
    return (
        select(AgentVersion)
        .join(Agent, AgentVersion.agent_id == Agent.id)
        .where(
            and_(
                AgentVersion.subdomain == subdomain,
                AgentVersion.id == Agent.published_version_id,
            )
        )
        .options(
            selectinload(AgentVersion.agent)
            .selectinload(Agent.published_version)
            .selectinload(AgentVersion.integrations),
            selectinload(AgentVersion.prompts),
        )
    )


def __build_get_agent_version_by_name_query(name: str):
    return (
        select(AgentVersion)
        .join(Agent, AgentVersion.agent_id == Agent.id)
        .where(and_(AgentVersion.name == name, Agent.is_deleted == false()))
        .options(
            selectinload(AgentVersion.agent)
            .selectinload(Agent.published_version)
            .selectinload(AgentVersion.integrations),
            selectinload(AgentVersion.prompts),
        )
    )


class GetRandomPublishedAgents(Message[list[AgentDisplayInfo]]):
    pass


def get_random_published_agents(request: GetRandomPublishedAgents, session: Session):
    agents = session.execute(__build_get_random_published_agents_query()).scalars().all()
    return [CAST(AgentDisplayInfo, agent.to_published_dict()) for agent in agents]


def __build_get_random_published_agents_query():
    return (
        select(Agent)
        .where(Agent.published_version_id.is_not(None))
        .where(Agent.is_deleted == false())
        .order_by(func.random())
        .limit(10)
        .options(selectinload(Agent.published_version).selectinload(AgentVersion.integrations))
    )
