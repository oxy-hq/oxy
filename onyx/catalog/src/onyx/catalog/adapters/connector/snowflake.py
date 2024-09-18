from functools import cached_property
from typing import cast

import snowflake.connector
from onyx.catalog.adapters.connector.base import (
    AbstractConnector,
    ColumnMetadata,
    TableMetadata,
)
from onyx.catalog.models.connection import Column
from onyx.catalog.models.errors import FailedToConnect


class SnowflakeConnector(AbstractConnector):
    def __init__(
        self,
        organization_id: str,
        connection_id: str,
        connection_config: dict[str, str],
    ):
        super().__init__(organization_id, connection_id, connection_config)
        self._connection: snowflake.connector.SnowflakeConnection | None = None

    def connect(self):
        if self._connection is None:
            try:
                self._connection = snowflake.connector.connect(**self.connection_config)
            except Exception as e:
                raise FailedToConnect from e
        return self

    def close(self) -> None:
        self.connection.close()
        self._connection = None

    def query_runner(self, query: str):
        dict_cursor = self.connection.cursor(snowflake.connector.DictCursor)
        dict_cursor.execute(query)
        return cast(list[dict], dict_cursor.fetchall())

    @cached_property
    def columns(self) -> list[Column]:
        query = """
            SELECT column_name, data_type, is_nullable, column_default, table_schema, table_name, is_identity
            FROM information_schema.columns
            WHERE table_schema != 'INFORMATION_SCHEMA'
        """

        columns = []
        for result in self.query_runner(query):
            result = cast(ColumnMetadata, {k.lower(): v for k, v in result.items()})
            column = self.serialize_column(result)
            columns.append(column)

        return columns

    def generate_table_ddl(self, table_full_name: str):
        query = f"""
            SELECT GET_DDL('table', '{table_full_name}', TRUE) as DDL_QUERY
        """
        results = self.query_runner(query)
        return results[0]["DDL_QUERY"]

    def get_tables(self):
        query = """
            SELECT table_schema, table_name, table_type
            FROM information_schema.tables
            WHERE table_schema != 'INFORMATION_SCHEMA'
            ORDER BY table_schema, table_name
        """
        tables = []
        for result in self.query_runner(query):
            result = cast(TableMetadata, {k.lower(): v for k, v in result.items()})
            table_full_name = f"{result['table_schema']}.\"{result['table_name']}\""
            result["ddl"] = self.generate_table_ddl(table_full_name)
            table = self.serialize_table(result)
            table.columns = self.get_table_columns(table)
            tables.append(table)

        return tables

    def query(self, query: str):
        result = self.query_runner(query)
        return [tuple(row.values()) for row in result]
