from typing import Generic, TypeVar

from pydantic import BaseModel

ResponseType = TypeVar("ResponseType")
ResponseChunk = TypeVar("ResponseChunk")


class Event(BaseModel):
    pass


class Message(BaseModel, Generic[ResponseType]):
    pass


class Command(Message[ResponseType]):
    pass
