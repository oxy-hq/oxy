from functools import cached_property
from typing import TypedDict, cast
from uuid import UUID

from clickhouse_connect import get_client
from clickhouse_connect.driver.client import Client
from onyx.catalog.adapters.connector.base import AbstractConnector
from onyx.catalog.models.connection import Column, Table
from onyx.catalog.models.errors import FailedToConnect
from onyx.shared.models.utils import string_is_quoted


class TableMetadata(TypedDict):
    table_schema: str
    table_name: str
    table_type: str


class ColumnMetadata(TypedDict):
    table_schema: str
    table_name: str
    column_name: str
    column_default: str
    data_type: str
    is_nullable: str


class ClickhouseConnector(AbstractConnector):
    def __init__(
        self,
        organization_id: str,
        connection_id: str,
        connection_config: dict[str, str],
    ):
        super().__init__(organization_id, connection_id, connection_config)
        self._connection: Client | None = None

    @property
    def connection(self) -> Client:
        if self._connection is None:
            raise Exception("Not connected, call `.connect()")
        return self._connection

    def query_runner(self, query: str):
        query_result = self.connection.query(query)
        return query_result.named_results()

    def connect(self):
        if self._connection is None:
            try:
                self._connection = get_client(
                    host=self.connection_config["host"],
                    port=int(self.connection_config["port"]),
                    user=self.connection_config["username"],
                    password=self.connection_config["password"],
                    database=self.connection_config["database"],
                )
            except Exception as e:
                raise FailedToConnect("Failed to connect to Clickhouse") from e
        return self

    def close(self, *args):
        if self._connection:
            self._connection.close()
            self._connection = None

    @cached_property
    def columns(self) -> list[Column]:
        query = f"""
            SELECT column_name, data_type, is_nullable, column_default, table_schema, table_name
            FROM information_schema.columns
            WHERE table_schema = '{self.connection_config["database"]}'
        """

        columns = []
        for result in self.query_runner(query):
            result = cast(ColumnMetadata, {k.lower(): v for k, v in result.items()})
            column = self.serialize_column(result)
            columns.append(column)

        return columns

    @cached_property
    def column_map(self) -> dict[tuple[str, str], list[Column]]:
        result_map: dict[tuple[str, str], list[Column]] = {}
        for col in self.columns:
            result_map.setdefault(col.table_identity, [])
            result_map[col.table_identity].append(col)
        return result_map

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
        )

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

    def generate_table_ddl(self, table_full_name: str):
        query = f"""
            SHOW CREATE TABLE {table_full_name}
        """
        results = self.query_runner(query)
        return next(results)["statement"]

    def serialize_table(self, raw: TableMetadata) -> Table:
        table_full_name = f"{raw['table_schema']}.\"{raw['table_name']}\""
        return Table(
            organization_id=UUID(self.organization_id),
            connection_id=UUID(self.connection_id),
            table_catalog=self.connection_config["database"],
            table_schema=raw["table_schema"],
            table_name=raw["table_name"],
            is_view="VIEW" in raw["table_type"],
            ddl_query=self.generate_table_ddl(table_full_name),
        )

    def get_tables(self):
        query = f"""
            SELECT table_schema, table_name, table_type
            FROM information_schema.tables
            WHERE table_schema = '{self.connection_config["database"]}'
            ORDER BY table_schema, table_name
        """
        tables = []
        for result in self.query_runner(query):
            result = cast(TableMetadata, {k.lower(): v for k, v in result.items()})
            table = self.serialize_table(result)
            table.columns = self.get_table_columns(table)
            tables.append(table)

        return tables

    def query(self, query: str):
        query_result = self.connection.query(query)
        return query_result.result_rows
