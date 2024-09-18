from uuid import UUID

from onyx.chat.adapters.feedback_analytics import AbstractFeedbackAnalytics
from onyx.chat.models.errors import ResourceNotFoundException
from onyx.chat.models.feedback import Feedback, FeedbackType
from onyx.chat.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.models.base import Event, Message
from onyx.shared.services.message_bus import EventCollector


class CreateFeedback(Message[Feedback]):
    content: str
    feedback_type: FeedbackType
    user_id: str
    message_id: str


def create_feedback(request: CreateFeedback, uow: AbstractUnitOfWork, collector: EventCollector) -> Feedback:
    message = uow.messages.get_by_id(request.message_id)
    if not message:
        raise ResourceNotFoundException("Message not found")

    feedback = Feedback(
        content=request.content,
        feedback_type=request.feedback_type,
        user_id=request.user_id,
        message_id=request.message_id,
    )
    message.feedbacks.append(feedback)
    uow.commit()

    if feedback.id and feedback.message.trace_id:
        collector.publish(
            FeedbackChanged(
                id=feedback.id,
                feedback_type=feedback.feedback_type,
                trace_id=UUID(feedback.message.trace_id),
                comment=feedback.content,
            )
        )

    return feedback


class FeedbackChanged(Event):
    id: UUID
    feedback_type: FeedbackType
    trace_id: UUID
    comment: str


def feedback_changed(event: FeedbackChanged, feedback_analytics: AbstractFeedbackAnalytics):
    score = 0
    if event.feedback_type == FeedbackType.positive:
        score = 1
    elif event.feedback_type == FeedbackType.negative:
        score = -1

    feedback_analytics.score(
        score=score,
        id=str(event.id),
        trace_id=str(event.trace_id),
        comment=event.comment,
    )
