from typing import AsyncIterable
from uuid import UUID, uuid4

from onyx.chat.adapters.ai_client import AbstractAIClient
from onyx.chat.adapters.catalog_client import AbstractCatalogClient
from onyx.chat.models.channel import Channel
from onyx.chat.models.errors import ResourceNotFoundException
from onyx.chat.models.feedback import Feedback
from onyx.chat.models.message import Message as MessageModel
from onyx.chat.models.message import MessageStatus
from onyx.chat.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.adapters.notify import AbstractNotification
from onyx.shared.logging import get_logger
from onyx.shared.models.base import Event, Message
from onyx.shared.models.common import ChatContext, ChatMessage, StreamingChunk, StreamingTrace
from onyx.shared.models.pagination import PaginationMetadata, PaginationParams
from onyx.shared.services.message_bus import EventCollector
from pydantic import Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

logger = get_logger(__name__)

LIST_MESSAGE_PAGE_SIZE = 100


class ListMessages(Message[tuple[list[dict], PaginationMetadata]]):
    channel_id: UUID
    pagination: PaginationParams = Field(default=PaginationParams(page_size=LIST_MESSAGE_PAGE_SIZE))


class SavePreview(Message):
    agent_id: UUID
    query: str
    message: str
    created_by: UUID
    organization_id: UUID


def list_messages(request: ListMessages, session: Session):
    stmt = (
        select(MessageModel)
        .filter(MessageModel.channel_id == request.channel_id)
        .order_by(MessageModel.created_at.desc())
        .limit(request.pagination.page_size)
        .offset(request.pagination.offset)
    )
    count_stmt = select(func.count(MessageModel.id)).filter(MessageModel.channel_id == request.channel_id)
    total_count = session.scalar(count_stmt)
    messages = session.scalars(stmt).all()

    message_ids = [m.id for m in messages]
    feedbacks = session.query(Feedback).filter(Feedback.message_id.in_(message_ids)).all()

    mapped_data = {}
    for feedback in feedbacks:
        mapped_data[str(feedback.message_id)] = feedback.feedback_type

    messages_dict = []
    for message in messages:
        message_dict = message.to_dict()
        message_dict["feedback"] = mapped_data.get(str(message.id), "")
        messages_dict.append(message_dict)

    return messages_dict, PaginationMetadata(
        page=request.pagination.page,
        page_size=request.pagination.page_size,
        total_count=total_count,
    )


def save_preview(request: SavePreview, uow: AbstractUnitOfWork, session: Session):
    channel = uow.channels.get_active_agent_channel(request.agent_id, request.created_by)
    if not channel:
        channel = Channel(
            name=request.query,
            created_by=request.created_by,
            organization_id=request.organization_id,
            agent_id=request.agent_id,
        )
        uow.channels.add(channel)
        uow.commit()

    last_message = (
        session.query(MessageModel)
        .filter(MessageModel.channel_id == channel.id)
        .order_by(MessageModel.created_at.desc())
        .first()
    )

    user_message = MessageModel.user_message(
        content=request.query,
        user_id=request.created_by,
        channel_id=channel.id,
        parent_id=last_message.id if last_message else None,
    )
    uow.messages.add(user_message)

    ai_message = MessageModel.ai_message_for(user_message)
    ai_message.content = request.message
    ai_message.status = MessageStatus.success
    uow.messages.add(ai_message)
    channel.last_message_at = ai_message.created_at
    uow.commit()


class ChatWithAI(Message[AsyncIterable[MessageModel]]):
    content: str
    user_id: UUID
    user_email: str
    username: str
    channel_id: UUID
    parent_id: UUID | None = None
    answer_id: UUID | None = None


async def chat_with_ai(
    request: ChatWithAI,
    uow: AbstractUnitOfWork,
    catalog_client: AbstractCatalogClient,
    ai_client: AbstractAIClient,
    event_collector: EventCollector,
):
    channel = uow.channels.get_by_id(request.channel_id)
    if not channel:
        raise ResourceNotFoundException("Channel not found")

    if not channel.agent_id:
        raise ValueError("Agent ID is required")

    agent_info = await catalog_client.get_agent_info(channel.agent_id, published=True)
    if not agent_info:
        raise ResourceNotFoundException("Agent not found")

    is_regenerated = bool(request.answer_id)

    if is_regenerated and request.parent_id:
        user_message = uow.messages.get_by_id(request.parent_id)
        if not user_message:
            raise ResourceNotFoundException(f"Message not found for {request.parent_id}")
    else:
        user_message = MessageModel.user_message(
            content=request.content,
            user_id=request.user_id,
            channel_id=request.channel_id,
            parent_id=request.parent_id,
        )
        uow.messages.add(user_message)

    yield user_message

    if request.answer_id:
        ai_message = uow.messages.get_by_id(request.answer_id)
        if not ai_message:
            raise ResourceNotFoundException(f"Message not found for {request.answer_id}")
        ai_message.content = ""
    else:
        ai_message = MessageModel.ai_message_for(user_message)
        uow.messages.add(ai_message)

    channel.last_message_at = ai_message.created_at
    uow.commit()
    trace_url = None
    total_duration = None
    time_to_first_token = None
    # Flag to handle generator exit edge case
    is_generator_exit = False

    try:
        async for message_chunk in ai_client.stream(
            text=request.content,
            context=ChatContext(
                username=request.username,
                user_id=request.user_id,
                user_email=request.user_email,
                organization_id=channel.organization_id,
                channel_id=request.channel_id,
            ),
            chat_history=uow.messages.list_messages(request.channel_id),
            agent_info=agent_info,
            cite_sources=True,
            tracing_session_id=str(request.channel_id),
        ):
            if isinstance(message_chunk, StreamingTrace):
                ai_message.trace_id = message_chunk.trace_id
                trace_url = message_chunk.trace_url
                total_duration = message_chunk.total_duration
                time_to_first_token = message_chunk.time_to_first_token
                continue

            ai_message.apply_streaming_chunk(message_chunk)
            yield ai_message.to_chunk(message_chunk)
        ai_message.status = MessageStatus.success
    except GeneratorExit:
        logger.error("Generator exited", exc_info=True)
        is_generator_exit = True
        raise
    except Exception as e:
        logger.error(f"Error occurred when streaming: {str(e)}", exc_info=True)
        ai_message.status = MessageStatus.failure
    finally:
        ai_message.status = MessageStatus.failure if ai_message.status == MessageStatus.streaming else ai_message.status
        if not is_generator_exit:
            yield ai_message.to_chunk(StreamingChunk.content(text=""))
        uow.commit()
        event_collector.publish(
            StreamFinished(
                user_email=request.user_email,
                question=request.content,
                answer=ai_message.content,
                trace_url=trace_url,
                agent_name=agent_info.name,
                total_duration=total_duration,
                time_to_first_token=time_to_first_token,
            )
        )


class PreviewWithAI(Message[AsyncIterable[MessageModel]]):
    content: str
    organization_id: UUID | None
    user_id: UUID
    user_email: str
    username: str
    agent_id: UUID
    chat_history: list[ChatMessage]
    parent_id: UUID | None
    is_published: bool
    slack_thread_ts: str | None = None


async def preview_with_ai(
    request: PreviewWithAI,
    ai_client: AbstractAIClient,
    catalog_client: AbstractCatalogClient,
    event_collector: EventCollector,
):
    agent_info = await catalog_client.get_agent_info(request.agent_id, published=request.is_published)
    if not agent_info:
        raise ResourceNotFoundException("Agent not found")

    user_message = MessageModel.user_message(
        content=request.content,
        user_id=request.user_id,
        parent_id=request.parent_id,
    )
    user_message.id = uuid4()
    yield user_message

    ai_message = MessageModel.ai_message_for(user_message)
    ai_message.id = uuid4()
    tracing_session_id = None
    trace_url = None
    trace_duration = None
    time_to_first_token = None
    # Flag to handle generator exit edge case
    is_generator_exit = False

    # only trace when previewing published version
    if request.is_published:
        tracing_session_id = str(request.agent_id)

    try:
        async for message_chunk in ai_client.stream(
            text=request.content,
            context=ChatContext(
                username=request.username,
                user_id=request.user_id,
                user_email=request.user_email,
                organization_id=request.organization_id,
            ),
            chat_history=request.chat_history,
            agent_info=agent_info,
            cite_sources=not request.is_published,  # Only cite sources from dev agent
            tracing_session_id=tracing_session_id,
        ):
            if isinstance(message_chunk, StreamingTrace):
                ai_message.trace_id = message_chunk.trace_id
                trace_url = message_chunk.trace_url
                trace_duration = message_chunk.total_duration
                time_to_first_token = message_chunk.time_to_first_token
                continue

            ai_message.apply_streaming_chunk(message_chunk)
            yield ai_message.to_chunk(message_chunk)
        if request.slack_thread_ts:
            event_collector.publish(
                PreviewedWithAI(
                    content=ai_message.content,
                    slack_thread_ts=request.slack_thread_ts,
                    agent_name=agent_info.name,
                    trace_url=trace_url,
                    total_duration=trace_duration,
                    time_to_first_token=time_to_first_token,
                )
            )
        ai_message.status = MessageStatus.success
    except GeneratorExit:
        logger.error("Generator exited", exc_info=True)
        is_generator_exit = True
        raise
    except Exception as e:
        logger.error(f"Error occurred when streaming: {str(e)}", exc_info=True)
        ai_message.status = MessageStatus.failure
    finally:
        ai_message.status = MessageStatus.failure if ai_message.status == MessageStatus.streaming else ai_message.status
        if not is_generator_exit:
            yield ai_message.to_chunk(StreamingChunk.content(text=""))


class StreamFinished(Event):
    user_email: str
    agent_name: str
    question: str
    answer: str
    trace_url: str | None
    total_duration: float | None
    time_to_first_token: float | None


async def stream_finished(event: StreamFinished, notification: AbstractNotification):
    await notification.notify_generated(
        user_email=event.user_email,
        question=event.question,
        answer=event.answer,
        trace_url=event.trace_url,
        agent_name=event.agent_name,
        total_duration=event.total_duration,
        time_to_first_token=event.time_to_first_token,
    )


class PreviewedWithAI(Event):
    agent_name: str
    content: str
    slack_thread_ts: str
    trace_url: str | None
    total_duration: float | None
    time_to_first_token: float | None


async def previewed_with_ai(event: PreviewedWithAI, notification: AbstractNotification):
    await notification.notify_previewed(
        agent_name=event.agent_name,
        content=event.content,
        slack_thread_ts=event.slack_thread_ts,
        trace_url=event.trace_url,
        total_duration=event.total_duration,
        time_to_first_token=event.time_to_first_token,
    )
