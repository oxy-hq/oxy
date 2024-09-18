from typing import Literal, Union

from onyx.shared.models.constants import (
    ConnectionSlugChoices,
    IntegrationSlugChoices,
)
from pydantic import BaseModel, Field, RootModel
from typing_extensions import Annotated


class SalesforceConfiguration(BaseModel):
    slug: Literal[IntegrationSlugChoices.salesforce]
    refresh_token: str


class GmailConfiguration(BaseModel):
    slug: Literal[IntegrationSlugChoices.gmail]
    refresh_token: str
    query: str = ""


class SlackConfiguration(BaseModel):
    slug: Literal[IntegrationSlugChoices.slack]
    token: str


class NotionConfiguration(BaseModel):
    slug: Literal[IntegrationSlugChoices.notion]
    token: str


class FileConfiguration(BaseModel):
    slug: Literal[IntegrationSlugChoices.file]
    path: str


class IntegrationConfiguration(RootModel):
    root: Annotated[
        SalesforceConfiguration | GmailConfiguration | SlackConfiguration | NotionConfiguration | FileConfiguration,
        Field(discriminator="slug"),
    ]


class SnowflakeConfiguration(BaseModel):
    slug: Literal[ConnectionSlugChoices.snowflake]
    account: str
    user: str
    password: str
    warehouse: str
    database: str
    role: str | None


class BigQueryConfiguration(BaseModel):
    slug: Literal[ConnectionSlugChoices.bigquery]
    database: str
    dataset: str
    credentials_key: dict


class ClickhouseConfiguration(BaseModel):
    slug: Literal[ConnectionSlugChoices.clickhouse]
    host: str
    port: int
    username: str
    password: str
    database: str


class ConnectionConfiguration(RootModel):
    root: Annotated[
        Union[SnowflakeConfiguration, ClickhouseConfiguration, BigQueryConfiguration],
        Field(discriminator="slug"),
    ]
