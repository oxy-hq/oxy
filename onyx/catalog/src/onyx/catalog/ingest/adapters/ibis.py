from contextlib import asynccontextmanager

from clickhouse_connect.driver.exceptions import DatabaseError
from ibis.backends.clickhouse import Backend
from ibis.expr.api import memtable, schema
from onyx.catalog.ingest.base.context import IngestContext, StreamContext
from onyx.catalog.ingest.base.sink import StagingSink
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from onyx.shared.services.dispatcher import AbstractDispatcher
from sqlglot.expressions import (
    ColumnDef,
    Create,
    EngineProperty,
    Order,
    Ordered,
    Properties,
    Schema,
    Tuple,
    column,
    table_,
    to_identifier,
)


class IbisSink(Logged, StagingSink):
    def __init__(self, connection: Backend, dispatcher: AbstractDispatcher) -> None:
        self.connection = connection
        self.dispatcher = dispatcher

    @classmethod
    @asynccontextmanager
    async def connect(cls, context: IngestContext, dispatcher: AbstractDispatcher, config: OnyxConfig):
        db_kwargs = {
            "host": config.clickhouse.host,
            "port": config.clickhouse.port,
            "user": config.clickhouse.username,
            "password": config.clickhouse.password,
            "secure": config.clickhouse.secure,
        }
        catalog_name = context.identity.staging_schema
        try:
            connection = Backend()
            connection.do_connect(
                **db_kwargs,
                database=catalog_name,
            )
        except DatabaseError as exc:
            if cls.__is_database_not_found(exc):
                connection.do_connect(
                    **db_kwargs,
                    database=config.clickhouse.database,
                )
                await dispatcher.dispatch(connection.create_database, name=catalog_name, force=True)
                connection.disconnect()
                connection.do_connect(
                    **db_kwargs,
                    database=catalog_name,
                )
            else:
                raise exc
        try:
            sink = cls(connection, dispatcher)
            yield sink
        finally:
            connection.disconnect()

    async def create_schema(self, context):
        code = self.__ddl_create(context)
        sql = code.sql(self.connection.name)
        await self.dispatcher.dispatch(self.connection.raw_sql, query=sql)

    async def _sink(self, context, records):
        table_name = context.stg_table_name
        rows = memtable(
            data=records,
            schema=schema(context.properties),
            name=table_name,
        ).to_pyarrow()
        self.log.info(f"Inserting {len(records)} records into {table_name}")
        await self.dispatcher.dispatch(self.connection.insert, name=table_name, obj=rows)

    @classmethod
    def __is_database_not_found(cls, exc: DatabaseError):
        try:
            return exc.args[0].split("\n ")[1].startswith("Code: 81")
        except IndexError:
            return False

    def __ddl_create(self, context: StreamContext):
        ibis_schema = schema(context.properties)
        stg_table = table_(context.stg_table_name)
        this = Schema(
            this=stg_table,
            expressions=[
                ColumnDef(
                    this=to_identifier(name, quoted=self.connection.compiler.quoted),
                    kind=self.connection.compiler.type_mapper.from_ibis(typ),
                )
                for name, typ in ibis_schema.items()
            ],
        )
        properties = [
            EngineProperty(this=to_identifier("MergeTree", quoted=False)),
            Order(expressions=[Ordered(this=Tuple(expressions=list(map(column, context.key_properties or ()))))]),
        ]
        code = Create(
            this=this,
            kind="TABLE",
            replace=context.rewrite,
            expression=None,
            properties=Properties(expressions=properties),
            exists=not context.rewrite,
        )
        return code
