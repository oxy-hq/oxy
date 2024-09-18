import uuid

import pytest
from onyx.chat.adapters.ai_client import FakeAIClient
from onyx.chat.features.chat import ChatWithAI
from onyx.chat.models.channel import Channel
from onyx.chat.models.errors import ResourceNotFoundException
from onyx.shared.services.base import Service
from sqlalchemy.orm import Session


@pytest.mark.asyncio(scope="session")
async def test_chat_with_ai(ai_client: FakeAIClient, service: Service, chat_channel: Channel):
    request = ChatWithAI(
        content="Hello",
        user_id=uuid.uuid4(),
        user_email="test@fake",
        username="test",
        channel_id=chat_channel.id,
    )

    content = ""
    async for message in service.handle_generator(request):
        if message.is_ai_message:
            content += message.content

    assert content == ai_client.current_message


@pytest.mark.asyncio(scope="session")
async def test_chat_with_ai_channel_not_found(service: Service):
    with pytest.raises(ResourceNotFoundException):
        request = ChatWithAI(
            content="Hello",
            user_id=uuid.uuid4(),
            user_email="test@fake",
            username="test",
            channel_id=uuid.uuid4(),
        )

        async for _message in service.handle_generator(request):
            ...


@pytest.mark.asyncio(scope="session")
async def test_chat_with_ai_agent_id_required(service: Service, chat_channel: Channel, sqlalchemy_session: Session):
    with pytest.raises(ValueError):
        chat_channel.agent_id = None
        sqlalchemy_session.commit()
        request = ChatWithAI(
            content="Hello",
            user_id=uuid.uuid4(),
            user_email="test@fake",
            username="test",
            channel_id=chat_channel.id,
        )

        async for _message in service.handle_generator(request):
            ...


@pytest.mark.asyncio(scope="session")
async def test_chat_with_ai_agent_not_found(service: Service, chat_channel: Channel, sqlalchemy_session: Session):
    with pytest.raises(ResourceNotFoundException):
        chat_channel.agent_id = uuid.uuid4()
        sqlalchemy_session.commit()
        request = ChatWithAI(
            content="Hello",
            user_id=uuid.uuid4(),
            user_email="test@fake",
            username="test",
            channel_id=chat_channel.id,
        )

        async for _message in service.handle_generator(request):
            ...
