from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from onyx.shared.services.base import Service


class AbstractOnyxApp(ABC):
    catalog: "Service"
    chat: "Service"
    ai: "Service"

    @abstractmethod
    async def teardown(self):
        pass
