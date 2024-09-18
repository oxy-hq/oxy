from onyx.shared.adapters.notify import AbstractNotification
from onyx.shared.models.base import Message


class Searched(Message[str]):
    query: str
    result: list
    user_email: str


async def searched(request: Searched, notification: AbstractNotification):
    return await notification.notify_searched(
        user_email=request.user_email,
        query=request.query,
        result=request.result,
    )
