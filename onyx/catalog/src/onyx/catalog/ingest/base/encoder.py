from abc import ABC, abstractmethod


class AbstractEncoder(ABC):
    @abstractmethod
    async def encode(self, chunks: list[str]) -> dict[str, list[float]]:
        pass
