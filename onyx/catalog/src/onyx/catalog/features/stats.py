from uuid import UUID

from onyx.catalog.models.agent import Agent
from onyx.catalog.models.user_agent_like import UserAgentLike
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.catalog.utils.mock import get_buffer_numbers
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Command, Message
from sqlalchemy import func, select
from sqlalchemy.orm import Session

logger = get_logger(__name__)


class GetUserAgentLike(Message[UserAgentLike]):
    user_id: UUID
    agent_id: UUID


def get_user_agent_like(request: GetUserAgentLike, session: Session) -> UserAgentLike:
    stmt = (
        select(UserAgentLike)
        .where(UserAgentLike.user_id == request.user_id)
        .where(UserAgentLike.agent_id == request.agent_id)
    )
    user_agent_like = session.scalars(stmt).one_or_none()
    return user_agent_like


class CountLikeByAgent(Message[int]):
    agent_id: UUID


def count_like_by_agent(request: CountLikeByAgent, session: Session):
    agent = session.execute(select(Agent).where(Agent.id == request.agent_id)).scalar_one()
    stmt = select(func.count(UserAgentLike.id)).where(UserAgentLike.agent_id == request.agent_id)
    user_agent_like_count = session.scalar(stmt)
    likes_buffer, _ = get_buffer_numbers(request.agent_id, agent.organization_id)
    user_agent_like_count += likes_buffer
    return user_agent_like_count


class CreateUserAgentLike(Command[UserAgentLike]):
    user_id: UUID
    agent_id: UUID


def create_user_agent_like(request: CreateUserAgentLike, uow: AbstractUnitOfWork):
    return uow.user_agent_like.create_user_agent_like(user_id=request.user_id, agent_id=request.agent_id)


class DeleteUserAgentLike(Command[None]):
    user_id: UUID
    agent_id: UUID


def delete_user_agent_like(request: DeleteUserAgentLike, uow: AbstractUnitOfWork):
    return uow.user_agent_like.delete_user_agent_like(user_id=request.user_id, agent_id=request.agent_id)
