from uuid import UUID

from onyx.chat.adapters.catalog_client import AbstractCatalogClient
from onyx.chat.models.channel import Channel
from onyx.chat.models.errors import ResourceNotFoundException
from onyx.chat.models.message import Message as ChatMessage
from onyx.chat.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Message
from onyx.shared.models.pagination import PaginationMetadata, PaginationParams
from pydantic import Field
from sqlalchemy import false, func, select
from sqlalchemy.orm import Session

logger = get_logger(__name__)


class GetChannel(Message[Channel]):
    id: UUID


def get_channel(request: GetChannel, session: Session):
    channel = session.get(Channel, request.id)
    if not channel:
        raise ResourceNotFoundException("Channel not found")
    return channel


class GetChannelByAgent(Message[Channel]):
    agent_subdomain: str
    created_by: UUID


async def get_channel_by_agent(request: GetChannelByAgent, session: Session, catalog_client: AbstractCatalogClient):
    agent_id = await catalog_client.get_agent_id_by_subdomain(request.agent_subdomain)
    channel = (
        session.query(Channel)
        .filter(Channel.agent_id == agent_id)
        .filter(Channel.created_by == request.created_by)
        .filter(Channel.is_deleted == false())  # noqa: E712   NOTE: using `is False` make the wrong query
        .first()
    )
    return channel


class ListChannels(Message[tuple[list[dict], PaginationMetadata]]):
    created_by: UUID | None = None
    organization_id: UUID | None = None
    is_public: bool | None = None
    agent_id: UUID | None = None
    pagination: PaginationParams = Field(default=PaginationParams())


async def list_channels(request: ListChannels, session: Session, catalog_client: AbstractCatalogClient):
    filters = [Channel.is_deleted == false()]
    if request.created_by:
        filters.append(Channel.created_by == request.created_by)
    if request.organization_id:
        filters.append(Channel.organization_id == request.organization_id)
    if request.is_public:
        filters.append(Channel.is_public == request.is_public)
    if request.agent_id:
        filters.append(Channel.agent_id == request.agent_id)

    pagination = request.pagination
    paginated_stmt = (
        select(Channel)
        .filter(*filters)
        .order_by(
            Channel.last_message_at.desc(),
        )
        .limit(pagination.page_size)
        .offset((pagination.page - 1) * pagination.page_size)
    )
    channels = session.scalars(paginated_stmt).all()
    count_stmt = select(func.count(Channel.id)).filter(*filters)
    total_count = session.scalar(count_stmt)

    agent_ids = []
    for channel in channels:
        if channel.agent_id:
            agent_ids.append(channel.agent_id)

    published_agents = await catalog_client.get_published_versions_by_agent_ids(agent_ids)

    channels_with_agent_info: list[dict] = []
    for channel in channels:
        channel_dict = channel.to_dict()
        agent = None
        if channel.agent_id:
            agent = published_agents.get(channel.agent_id, None)
        channel_dict["agent_name"] = agent["name"] if agent else None
        channel_dict["agent_avatar"] = agent["avatar"] if agent else None
        channel_dict["agent_subdomain"] = agent["subdomain"] if agent else None
        channel_dict["agent_is_deleted"] = agent["is_deleted"] if agent else None
        channels_with_agent_info.append(channel_dict)

    return channels_with_agent_info, PaginationMetadata(
        page=pagination.page, page_size=pagination.page_size, total_count=total_count
    )


class CountMessageByAgent(Message[int]):
    agent_id: UUID


def count_message_by_agent(request: CountMessageByAgent, session: Session):
    try:
        stmt = (
            select(func.count(ChatMessage.id))
            .join(Channel, Channel.id == ChatMessage.channel_id)
            .filter(Channel.agent_id == request.agent_id)
            .filter(ChatMessage.is_ai_message == false())
        )
        count = session.scalar(stmt)
        return count
    except Exception as e:
        logger.error(f"Error occurred when counting message: {str(e)}", exc_info=True)
        return 0


class CountMessageByAgents(Message[dict[UUID, int]]):
    agent_ids: list[UUID]


def count_message_by_agents(request: CountMessageByAgents, session: Session):
    try:
        stmt = (
            select(Channel.agent_id, func.count(ChatMessage.id))
            .join(Channel, Channel.id == ChatMessage.channel_id)
            .filter(Channel.agent_id.in_(request.agent_ids))
            .filter(ChatMessage.is_ai_message == false())
            .group_by(Channel.agent_id)
        )
        counts = session.execute(stmt).fetchall()
        return dict(counts)
    except Exception as e:
        logger.error(f"Error occurred when counting message: {str(e)}", exc_info=True)
        return {}


class CreateChannel(Message[dict]):
    name: str
    created_by: UUID
    organization_id: UUID
    agent_id: UUID | None = None


def create_channel(request: CreateChannel, uow: AbstractUnitOfWork) -> dict:
    channel = Channel(
        name=request.name,
        created_by=request.created_by,
        organization_id=request.organization_id,
        agent_id=request.agent_id,
    )

    if request.agent_id:
        existed_channel = uow.channels.get_active_agent_channel(request.agent_id, request.created_by)
        if existed_channel:
            return existed_channel.to_dict()

    uow.channels.add(channel)
    uow.commit()
    return channel.to_dict()


class UpdateChannel(Message[dict]):
    id: UUID
    name: str


def update_channel(request: UpdateChannel, uow: AbstractUnitOfWork) -> dict:
    channel = uow.channels.get_by_id(request.id)
    if not channel:
        raise ResourceNotFoundException("Channel not found")
    channel.name = request.name
    uow.commit()
    return channel.to_dict()


class DeleteChannel(Message[bool]):
    id: UUID


def delete_channel(request: DeleteChannel, uow: AbstractUnitOfWork):
    channel = uow.channels.get_by_id(request.id)
    if not channel:
        raise ResourceNotFoundException("Channel not found")
    channel.is_deleted = True
    uow.commit()
    return channel.is_deleted


class GetSampleChannels(Message):
    pass


async def get_sample_channels(
    request: GetSampleChannels, uow: AbstractUnitOfWork, catalog_client: AbstractCatalogClient
):
    agents = await catalog_client.get_random_published_agents()
    channels = []
    for agent in agents:
        channel_data = {
            "name": agent["name"],
            "agent_name": agent["name"],
            "agent_avatar": agent["avatar"],
            "agent_subdomain": agent["subdomain"],
            "agent_is_deleted": False,
        }
        channels.append(channel_data)
    return channels
