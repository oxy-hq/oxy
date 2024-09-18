import logging
import sys

logging.basicConfig(stream=sys.stdout, level=logging.INFO)


def configure_sentry(dsn: str, environment: str, logger: logging.Logger = logging.getLogger(__name__)) -> None:
    try:
        import sentry_sdk  # noqa: E402

        logger.info("Configuring sentry")

        sentry_sdk.init(
            dsn=dsn,
            enable_tracing=False,
            traces_sample_rate=0,
            send_default_pii=True,
            attach_stacktrace=True,
            auto_enabling_integrations=True,
            profiles_sample_rate=0,
            environment=environment,
        )
        logger.info("Sentry configured")
    except ImportError:
        logger.info("Sentry not configured")
    except Exception as e:
        logger.error(f"Sentry configuration failed: {e}")
