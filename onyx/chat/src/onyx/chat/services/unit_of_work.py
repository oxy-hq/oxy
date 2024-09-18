from abc import ABC

from onyx.chat.adapters.repositories.channel import (
    AbstractChannelRepository,
    AbstractMessageRepository,
    ChannelRepository,
    MessageRepository,
)
from onyx.shared.adapters.orm.mixins import SQLUnitOfWorkMixin
from onyx.shared.services.unit_of_work import AbstractUnitOfWork as _AbstractUnitOfWork


class AbstractUnitOfWork(_AbstractUnitOfWork, ABC):
    channels: AbstractChannelRepository
    messages: AbstractMessageRepository


class UnitOfWork(SQLUnitOfWorkMixin, AbstractUnitOfWork):
    def _init_repositories(self, session):
        self.channels = ChannelRepository(session)
        self.messages = MessageRepository(session)

    @property
    def session(self):
        return self._session
