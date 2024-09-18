import uuid

import pytest
from langchain_core.language_models import FakeListChatModel
from onyx.ai.features.agent import StreamRequest
from onyx.shared.models.common import AgentInfo, ChatContext, StreamingChunk
from onyx.shared.services.base import Service


@pytest.mark.asyncio(scope="session")
async def test_stream(service: Service, fake_chat_model: FakeListChatModel):
    request = StreamRequest(
        text="Hello",
        context=ChatContext(
            organization_id=uuid.uuid4(),
            username="test",
            user_email="test@fake",
            channel_id=uuid.uuid4(),
            user_id=uuid.uuid4(),
        ),
        chat_history=[],
        agent_info=AgentInfo(
            name="test", instructions="test", description="test", knowledge="", data_sources=[], training_prompts=[]
        ),
        cite_sources=False,
        tracing_session_id=str(uuid.uuid4()),
    )
    content = ""
    async for chunk in service.handle_generator(request):
        if isinstance(chunk, StreamingChunk):
            content += chunk.text

    assert content == fake_chat_model.responses[fake_chat_model.i - 1]
