from dataclasses import dataclass
from typing import Literal, TypeVar

from onyx.shared.models.constants import IntegrationSlugChoices
from pydantic import AliasGenerator, BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel
from slugify import slugify
from typing_extensions import Annotated, TypedDict

Cursor = TypeVar("Cursor")
Request = TypeVar("Request")
Response = TypeVar("Response")


@dataclass
class Interval:
    start: int
    end: int


class Record(BaseModel):
    model_config = ConfigDict(
        alias_generator=AliasGenerator(
            validation_alias=to_camel,
        )
    )


class EmbeddingRecord(TypedDict):
    id: str
    title: str
    metadata: list[str]
    timestamp: int
    chunks: list[str]
    embeddings: dict[str, list[float]]


class Identity(BaseModel):
    slug: IntegrationSlugChoices
    namespace_id: str
    datasource_id: str

    @property
    def staging_schema(self):
        return "onyx__" + slugify(self.namespace_id, separator="_")

    def staging_table(self, name: str):
        return f"{self.slug}__{name}__" + slugify(self.datasource_id, separator="_")

    @property
    def embed_namespace(self):
        return "onyx__" + slugify(self.namespace_id, separator="_")

    @property
    def embed_groupname(self):
        return f"{self.slug}__" + slugify(self.datasource_id, separator="_")


class OAuthConfig(BaseModel):
    auth_type: Literal["oauth"] = "oauth"
    headers: dict[str, str] = Field(default_factory=dict)
    endpoint: str
    client_id: str
    client_secret: str
    refresh_token: str


class APITokenConfig(BaseModel):
    auth_type: Literal["api_token"] = "api_token"
    token: str


AuthConfig = Annotated[
    OAuthConfig | APITokenConfig,
    Field(discriminator="auth_type"),
]
