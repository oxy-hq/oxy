from datetime import UTC, datetime
from uuid import UUID

from onyx.catalog.models.agent import Agent
from onyx.catalog.models.prompt import Prompt
from onyx.catalog.services.unit_of_work import AbstractUnitOfWork
from onyx.shared.models.base import Command, Message
from sqlalchemy import select
from sqlalchemy.orm import Session


class ListAgentPrompts(Message[list[dict]]):
    agent_id: UUID


def list_agent_prompts(request: ListAgentPrompts, session: Session):
    stmt = select(Agent).where(Agent.id == request.agent_id)
    agent = session.scalars(stmt).one_or_none()
    if not agent or not agent.dev_version:
        raise Exception("Agent not found")

    prompts = [prompt.to_dict() for prompt in agent.dev_version.prompts]
    prompts.sort(key=lambda x: x["created_at"])
    return prompts


class AddPrompt(Command[dict]):
    message: str
    agent_id: UUID


def add_prompt(request: AddPrompt, uow: AbstractUnitOfWork):
    agent = uow.agents.get_by_id(request.agent_id)
    if not agent:
        raise ValueError("Agent not found")
    prompt = Prompt(message=request.message, agent_version_id=agent.dev_version_id, is_recommended=False, sources=[])
    new_prompt = uow.prompts.add(prompt)
    uow.commit()

    return new_prompt.to_dict()


class UpdatePrompt(Command):
    id: UUID
    message: str
    source_ids: list[UUID]
    is_recommended: bool


def update_prompt(request: UpdatePrompt, uow: AbstractUnitOfWork):
    prompt = uow.prompts.get_by_id(request.id)
    agent = uow.agents.get_by_id(prompt.agent_version.agent_id)
    if not prompt:
        raise ValueError("Prompt not found")
    prompt.message = request.message
    prompt.is_recommended = request.is_recommended
    dev_integrations = agent.dev_version.integrations
    integrations = [integration for integration in dev_integrations if integration.id in request.source_ids]

    # Don't need to check if all command.source_ids are found,
    # will let integrations are source of truth,
    # because some time user remove integration that already added to prompt
    prompt.sources = integrations

    # need manual update here because by default if any source are changed, it won't update prompt.updated_at
    prompt.updated_at = datetime.now(UTC)

    uow.commit()

    return prompt.to_dict()


class DeletePrompt(Command):
    id: UUID


def delete_prompt(request: DeletePrompt, uow: AbstractUnitOfWork):
    prompt = uow.prompts.get_by_id(request.id)
    if not prompt:
        raise ValueError("Prompt not found")
    uow.prompts.delete(prompt.id)
    uow.commit()
    return True
