from onyx.ai.adapters.warehouse_client import AbstractWarehouseClient
from onyx.ai.base.models import FunctionDefinition
from onyx.ai.base.tools import Tool
from onyx.shared.models.common import Column, DataSource, Table
from pydantic import BaseModel
from slugify import slugify


class SQLQueryInput(BaseModel):
    query: str


class SQLQueryTool(Tool):
    def __init__(
        self,
        identifier: str,
        database_schema: str,
        description: str,
        warehouse_client: AbstractWarehouseClient,
        run_query_kwargs: dict | None = None,
    ) -> None:
        self.identifier = identifier
        self.definition = FunctionDefinition(
            name=f"execute_sql_query_{slugify(self.identifier)}",
            description=description,
            parameters={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": f"""
                                SQL query extracting info to answer the user's question.
                                SQL should be written using this database schema:
                                {database_schema}
                                The query should be returned in plain text, not in JSON.
                                """,
                    }
                },
                "required": ["query"],
            },
        )
        self.warehouse_client = warehouse_client
        self.run_query_kwargs = run_query_kwargs or {}

    @property
    def name(self):
        return self.definition.name

    async def _run(self, parameters: dict) -> str:
        query = SQLQueryInput(**parameters)
        self.log.info(f"Running SQL query tool {self.name} with parameters: {parameters}")
        results = await self.warehouse_client.run_query(query=query.query, **self.run_query_kwargs)
        return str(results)

    @classmethod
    def from_datasource(cls, data_source: DataSource, warehouse_client: AbstractWarehouseClient) -> "SQLQueryTool":
        def column_description(column: Column) -> str:
            return f"{column['name']} ({column['type']})"

        def table_description(table: Table) -> str:
            return f"Table: {table['schema']}.{table['name']}\nColumns: {', '.join([column_description(c) for c in table['columns']])}"

        database_schema = "\n----\n".join([table_description(t) for t in data_source["source_tables"]])
        semantic_definitions = data_source.get("metadata", {}).get("semanticDefinitions")
        few_shots = []
        if semantic_definitions:
            for definition in semantic_definitions:
                query = definition.get("query", "")
                description = definition.get("description", "")
                if query and description:
                    few_shots.append(f"- {description}\n```{query}```")

        data_source_description = f"Data source: {data_source['name']}"
        if few_shots:
            few_shots_description = "\n----\n".join(few_shots)
            data_source_description += f"\nExamples:\n{few_shots_description}"

        return cls(
            identifier=data_source["name"],
            description=data_source_description,
            database_schema=database_schema,
            warehouse_client=warehouse_client,
            run_query_kwargs={
                "organization_id": data_source["organization_id"],
                "connection_id": data_source["id"],
            },
        )
