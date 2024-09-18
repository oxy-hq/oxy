import abc
from functools import cached_property
from typing import Iterable, Self, TypedDict
from uuid import UUID

from onyx.catalog.models.connection import Column, Table
from onyx.shared.models.utils import string_is_quoted


class ColumnMetadata(TypedDict):
    table_schema: str
    table_name: str
    column_name: str
    column_default: str
    data_type: str
    is_nullable: str
    is_identity: str


class TableMetadata(TypedDict):
    table_schema: str
    table_name: str
    table_type: str
    ddl: str


class AbstractConnector(abc.ABC):
    def __init__(
        self,
        organization_id: str,
        connection_id: str,
        connection_config: dict[str, str],
    ):
        self.__organization_id = organization_id
        self.__connection_id = connection_id
        self.__connection_config = connection_config

    @property
    def organization_id(self):
        return self.__organization_id

    @property
    def connection_id(self):
        return self.__connection_id

    @property
    def connection_config(self):
        return self.__connection_config

    @property
    def connection(self):
        if self._connection is None:
            raise Exception("Not connected, call `.connect()")
        return self._connection

    @cached_property
    def column_map(self) -> dict[tuple[str, str], list[Column]]:
        result_map: dict[tuple[str, str], list[Column]] = {}
        for col in self.columns:
            result_map.setdefault(col.table_identity, [])
            result_map[col.table_identity].append(col)
        return result_map

    def get_table_columns(self, table: Table):
        table_id = table.identity
        if table_id in self.column_map:
            return self.column_map[table_id]

        schema = table_id[0] if string_is_quoted(table_id[0]) else table_id[0].upper()
        name = table_id[1] if string_is_quoted(table_id[1]) else table_id[1].upper()

        table_id = (schema, name)
        if table_id in self.column_map:
            return self.column_map[table_id]
        else:
            raise Exception(f"No columns found for table with schema={schema} and name={name}")

    def serialize_table(self, raw: TableMetadata):
        return Table(
            organization_id=UUID(self.organization_id),
            connection_id=UUID(self.connection_id),
            table_catalog=self.connection_config["database"],
            table_schema=raw["table_schema"],
            table_name=raw["table_name"],
            is_view="VIEW" in raw["table_type"],
            ddl_query=raw["ddl"],
        )

    def serialize_column(self, raw: ColumnMetadata):
        return Column(
            organization_id=UUID(self.organization_id),
            connection_id=UUID(self.connection_id),
            table_catalog=self.connection_config["database"],
            table_schema=raw["table_schema"],
            table_name=raw["table_name"],
            column_name=raw["column_name"],
            column_default=raw["column_default"],
            data_type=raw["data_type"],
            is_nullable=raw["is_nullable"] == "YES",
            is_pk=raw["is_identity"] == "YES",
        )

    def __enter__(self):
        return self.connect()

    def __exit__(self, *args):
        self.close()

    def test_connection(self) -> bool:
        try:
            with self:
                self.query("SELECT 1")
                return True
        except Exception:
            return False

    @abc.abstractmethod
    def connect(self) -> Self:
        ...

    @abc.abstractmethod
    def close(self, *args):
        ...

    @abc.abstractmethod
    def get_tables(self) -> Iterable[Table]:
        ...

    @abc.abstractmethod
    def query(self, query: str):
        ...
