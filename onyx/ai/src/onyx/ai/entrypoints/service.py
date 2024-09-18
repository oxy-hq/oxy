from onyx.ai.features.agent import stream
from onyx.shared.services.base import Service

ai_service = Service("ai")
ai_service.with_handlers(stream)
