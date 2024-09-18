import uuid

from onyx.catalog.models.agent_version import AgentVersion
from onyx.catalog.models.base import CatalogModel
from onyx.catalog.models.integration import Integration
from onyx.catalog.models.prompt_integration import PromptIntegration
from onyx.shared.models.common import TrainingPrompt, TrainingPromptSource
from sqlalchemy import ForeignKey
from sqlalchemy.orm import Mapped, mapped_column, relationship


class Prompt(CatalogModel):
    __tablename__ = "prompt"

    agent_version_id: Mapped[uuid.UUID] = mapped_column(ForeignKey(AgentVersion.id), index=True)
    agent_version: Mapped[AgentVersion] = relationship(
        AgentVersion, back_populates="prompts", foreign_keys=[agent_version_id]
    )
    is_recommended: Mapped[bool] = mapped_column(default=False)
    message: Mapped[str] = mapped_column(default="")
    sources: Mapped[list[Integration]] = relationship(back_populates="prompts", secondary=PromptIntegration.__table__)

    def to_dict(self, **kwargs):
        return {
            "id": self.id,
            "agent_id": self.agent_version.agent_id,
            "is_recommended": self.is_recommended,
            "message": self.message,
            "created_at": self.created_at,
            "source_ids": [source.id for source in self.sources],
        }

    def clone(self):
        return Prompt(
            is_recommended=self.is_recommended,
            message=self.message,
            sources=self.sources,
        )

    def to_training_prompt(self) -> TrainingPrompt:
        return TrainingPrompt(
            message=self.message,
            sources=[
                TrainingPromptSource(
                    id=str(source.id),
                    type=source.slug,
                    # TODO: filters, like notion page etc, go here
                    filters="",
                    target_embedding_table=source.target_embedding_table,
                )
                for source in self.sources
            ],
        )

    @property
    def is_changed(self):
        return self.updated_at > self.created_at
