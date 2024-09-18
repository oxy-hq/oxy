from onyx.chat.models.all import *  # noqa initilize all models before engine & session
from onyx.chat.features.channel import (
    count_message_by_agent,
    count_message_by_agents,
    create_channel,
    delete_channel,
    get_channel,
    get_channel_by_agent,
    get_sample_channels,
    list_channels,
    update_channel,
)
from onyx.chat.features.chat import (
    chat_with_ai,
    list_messages,
    preview_with_ai,
    previewed_with_ai,
    save_preview,
    stream_finished,
)
from onyx.chat.features.feedback import (
    create_feedback,
    feedback_changed,
)
from onyx.shared.services.base import Service

chat_service = Service("chat")
chat_service.with_handlers(
    # Channel
    count_message_by_agent,
    count_message_by_agents,
    create_channel,
    delete_channel,
    get_channel,
    get_channel_by_agent,
    list_channels,
    update_channel,
    # Feedback
    create_feedback,
    feedback_changed,
    # Chat
    list_messages,
    save_preview,
    chat_with_ai,
    preview_with_ai,
    stream_finished,
    previewed_with_ai,
    get_sample_channels,
)
