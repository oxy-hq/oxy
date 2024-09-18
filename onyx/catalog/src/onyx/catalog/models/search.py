from dataclasses import dataclass
from typing import Any
from uuid import UUID

from typing_extensions import TypedDict


class VespaDocument(TypedDict):
    id: str
    groupname: str
    relevance: float
    fields: dict[str, Any]


@dataclass
class AgentDocument:
    id: UUID
    name: str
    description: str
    conversation_starters: list[str]
    avatar: str
    subdomain: str
    match_features: dict[str, float] | None = None
    relevance: float = 0.0
    reason: str = ""

    def to_vespa_fields(self):
        return {
            "name": self.name,
            "description": self.description,
            "conversation_starters": self.conversation_starters,
            "metadata": [
                f"avatar==={self.avatar}",
                f"subdomain==={self.subdomain}",
            ],
        }

    def to_document(self) -> str:
        return f"Name: {self.name}\nDescription: {self.description}\nConversation starters: {' '.join(self.conversation_starters)}"

    @classmethod
    def from_vespa(cls, doc: VespaDocument) -> "AgentDocument":
        fields = doc["fields"]
        metadata = dict(
            entry.split("===")
            for entry in fields.get(
                "metadata",
                [],
            )
        )
        doc_id = doc["id"].split(":")[-1]
        return cls(
            id=UUID(doc_id),
            relevance=doc["relevance"],
            name=fields["name"],
            description=fields["description"],
            conversation_starters=fields.get("conversation_starters", []),
            avatar=metadata.get("avatar", ""),
            subdomain=metadata.get("subdomain", ""),
            match_features=fields.get("matchfeatures"),
        )
