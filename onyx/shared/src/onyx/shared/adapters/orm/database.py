import json
from contextlib import contextmanager
from typing import TYPE_CHECKING, Callable

from onyx.shared.config import DatabaseSettings
from sqlalchemy import Connection
from sqlalchemy import create_engine as _create_engine
from sqlalchemy.orm import Session, sessionmaker

if TYPE_CHECKING:
    from sqlalchemy import Engine


SessionFactoryFunc = Callable[[], Session]
ConnectionFactoryFunc = Callable[[], Connection]


def sqlalchemy_session_maker(engine: "Engine") -> Callable[[], Session]:
    return sessionmaker(autocommit=False, autoflush=False, expire_on_commit=False, bind=engine)


def json_serializer(obj):
    return json.JSONEncoder().encode(obj)


def create_engine(config: DatabaseSettings, echo=False):
    return _create_engine(
        url=config.connection_string,
        pool_size=config.pool_size,
        max_overflow=config.pool_max_overflow,
        json_serializer=json_serializer,
        echo=echo,
    )


def read_session_factory(engine: "Engine"):
    def connect():
        connection = engine.connect()
        session = Session(bind=connection)
        try:
            yield session
        finally:
            session.close()
            connection.close()

    return contextmanager(connect)
