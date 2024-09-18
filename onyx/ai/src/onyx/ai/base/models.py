from enum import StrEnum, auto

from pydantic import BaseModel
from pydantic.json_schema import JsonSchemaValue


class ReferenceSourceTypes(StrEnum):
    gmail = auto()
    table = auto()
    salesforce = auto()
    slack = auto()
    notion = auto()
    web = auto()


class FunctionDefinition(BaseModel):
    name: str
    description: str | None
    parameters: JsonSchemaValue
    strict: bool = False
