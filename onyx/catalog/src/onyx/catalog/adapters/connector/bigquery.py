from functools import cached_property
from typing import cast

import google.oauth2.service_account as google_service_account
from google.cloud import bigquery
from onyx.catalog.adapters.connector.base import (
    AbstractConnector,
    ColumnMetadata,
    TableMetadata,
)
from onyx.catalog.models.connection import (
    Column,
)
from onyx.catalog.models.errors import FailedToConnect


class BigQueryConnector(AbstractConnector):
    def __init__(
        self,
        organization_id: str,
        connection_id: str,
        connection_config: dict[str, str],
    ):
        super().__init__(organization_id, connection_id, connection_config)
        self._connection: bigquery.Client | None = None
        self._DEFAULT_SCOPES = [
            "https://www.googleapis.com/auth/bigquery",
            "https://www.googleapis.com/auth/drive",
        ]
        self._database = connection_config.get("database", "")
        self._schema = connection_config.get("dataset", "")

    def connect(self) -> "BigQueryConnector":
        if self._connection is None:
            try:
                credentials_key = self.connection_config.get("credentials_key")
                credentials = google_service_account.Credentials.from_service_account_info(
                    credentials_key,
                    scopes=self._DEFAULT_SCOPES,
                )
                self._connection = bigquery.Client(credentials=credentials)
            except Exception as e:
                raise FailedToConnect from e

        return self

    def close(self):
        self.connection.close()
        self._connection = None

    def query_runner(self, query: str):
        query_job = self._connection.query(query)
        return [dict(row.items()) for row in query_job]

    @cached_property
    def primary_keys(self) -> set:
        query = f"""
            SELECT tc.constraint_name, tc.table_name, ccu.column_name, tc.constraint_type
            FROM `{self._database}.{self._schema}.INFORMATION_SCHEMA.TABLE_CONSTRAINTS` tc
            JOIN `{self._database}.{self._schema}.INFORMATION_SCHEMA.CONSTRAINT_COLUMN_USAGE` ccu
            ON tc.constraint_name = ccu.constraint_name
            WHERE tc.constraint_type = 'PRIMARY KEY'
        """

        primary_keys = set()
        for result in self.query_runner(query):
            key = (result["table_name"], result["column_name"])
            primary_keys.add(key)

        return primary_keys

    @cached_property
    def columns(self) -> list[Column]:
        query = f"""
            SELECT column_name, data_type, is_nullable, column_default, table_schema, table_name
            FROM `{self._database}.{self._schema}.INFORMATION_SCHEMA.COLUMNS`
            WHERE table_schema != 'INFORMATION_SCHEMA'
        """

        columns = []
        for result in self.query_runner(query):
            result: ColumnMetadata = cast(ColumnMetadata, {k.lower(): v for k, v in result.items()})
            is_identity = (
                result["table_name"],
                result["column_name"],
            ) in self.primary_keys
            result["is_identity"] = "YES" if is_identity else "NO"
            column = self.serialize_column(result)
            columns.append(column)

        return columns

    def get_tables(self):
        query = f"""
            SELECT table_schema, table_name, table_type, ddl
            FROM `{self._database}.{self._schema}.INFORMATION_SCHEMA.TABLES`
            WHERE table_schema != 'INFORMATION_SCHEMA'
            ORDER BY table_schema, table_name
        """

        tables = []
        for result in self.query_runner(query):
            table_metadata = cast(TableMetadata, {k.lower(): v for k, v in result.items()})
            table = self.serialize_table(table_metadata)
            table.columns = self.get_table_columns(table)
            tables.append(table)

        return tables

    def query(self, query: str):
        result = self._connection.query(query)
        return list(result)
