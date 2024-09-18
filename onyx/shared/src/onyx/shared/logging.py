import orjson
import structlog
from structlog.typing import Processor


def configure_logging(log_level: int, local=False):
    shared_processors: list[Processor] = [
        structlog.contextvars.merge_contextvars,
        structlog.processors.add_log_level,
        structlog.processors.format_exc_info,
        structlog.processors.TimeStamper(fmt="iso", utc=True),
    ]

    if local:
        processors = [
            *shared_processors,
            structlog.dev.ConsoleRenderer(),
        ]
        logger_factory = structlog.PrintLoggerFactory()
    else:
        processors = [
            *shared_processors,
            structlog.processors.dict_tracebacks,
            structlog.processors.JSONRenderer(serializer=orjson.dumps),
        ]
        logger_factory = structlog.BytesLoggerFactory()

    structlog.configure(
        cache_logger_on_first_use=True,
        wrapper_class=structlog.make_filtering_bound_logger(log_level),
        processors=processors,
        logger_factory=logger_factory,
    )


def get_logger(source: str = "", **kwargs: str) -> structlog.stdlib.BoundLogger:
    logger = structlog.get_logger()
    return logger.bind(source=source, **kwargs)


class LogDescriptor:
    def __get__(self, instance: "Logged", obj_type: "type[Logged] | None" = None):
        attributes = instance.get_logging_attributes()
        return get_logger(instance.__class__.__name__, **attributes)


class Logged:
    log = LogDescriptor()

    def get_logging_attributes(self) -> dict[str, str]:
        return {}
