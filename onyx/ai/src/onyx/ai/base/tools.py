from abc import ABC, abstractmethod

from onyx.ai.base.models import FunctionDefinition
from onyx.shared.logging import Logged


class Tool(Logged, ABC):
    definition: FunctionDefinition

    @abstractmethod
    async def _run(self, parameters: dict) -> str:
        pass

    async def run(self, parameters: dict) -> str:
        try:
            return await self._run(parameters)
        except Exception as e:
            self.log.error(f"Tool {self.name} failed with error: {e}", exc_info=True)
            return f"Tool {self.name} failed with error: {e}"

    @property
    def name(self):
        return self.definition.name

    def to_spec(self):
        return {
            "type": "function",
            "function": self.definition.model_dump(),
        }


class NotFoundTool(Tool):
    def __init__(self, name: str) -> None:
        self.definition = FunctionDefinition(name=name, description="Tool not found", parameters={})

    async def _run(self, parameters: dict) -> str:
        return f"Tool {self.name} not found"


class ToolsRegistry:
    def __init__(self):
        self.__tools: dict[str, Tool] = {}

    def register(self, tool: Tool):
        if tool.name in self.__tools:
            raise ValueError(f"Tool {tool.name} already registered")
        self.__tools[tool.name] = tool

    def get(self, name: str):
        found = self.__tools.get(name)
        if found is not None:
            return found
        return NotFoundTool(name=name)

    def to_spec(self):
        return [tool.to_spec() for tool in self.__tools.values()]

    def __repr__(self):
        return f"<ToolsRegistry: {list(self.__tools.keys())}>"
