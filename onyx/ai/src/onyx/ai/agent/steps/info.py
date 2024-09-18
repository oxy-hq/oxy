from langchain_core.runnables import RunnableSerializable
from onyx.ai.base.builder import ChainInput
from onyx.shared.logging import Logged


class AgentInfoRunnable(Logged, RunnableSerializable[ChainInput, str]):
    def invoke(self, input, config):
        agent_info = input["agent_info"]
        return f"""---
Name: {agent_info.name}
Description: {agent_info.description}
Instruction: {agent_info.instructions}
Knowledge Sources: {agent_info.knowledge}
---
"""
