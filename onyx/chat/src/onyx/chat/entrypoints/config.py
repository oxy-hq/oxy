from onyx.app import AbstractOnyxApp
from onyx.chat.adapters.ai_client import AbstractAIClient, AIClient
from onyx.chat.adapters.catalog_client import AbstractCatalogClient, CatalogClient
from onyx.chat.adapters.feedback_analytics import (
    AbstractFeedbackAnalytics,
    ConsoleFeedbackAnalytics,
    LangfuseFeedbackAnalytics,
)
from onyx.chat.services.unit_of_work import AbstractUnitOfWork, UnitOfWork
from onyx.shared.adapters.notify import AbstractNotification, ConsoleNotification, SlackNotification
from onyx.shared.adapters.orm.database import create_engine, read_session_factory, sqlalchemy_session_maker
from onyx.shared.adapters.orm.mixins import sql_uow_factory
from onyx.shared.config import OnyxConfig
from onyx.shared.models.handlers import DependencyRegistration
from sqlalchemy.orm import Session


def config_mapper(config: OnyxConfig, app: AbstractOnyxApp):
    engine = create_engine(config.database)
    write_session_factory = sqlalchemy_session_maker(engine)
    feedback_analytics_cls = LangfuseFeedbackAnalytics
    if config.stage.is_local():
        feedback_analytics_cls = ConsoleFeedbackAnalytics
    notification_cls = SlackNotification
    if config.stage.is_local():
        notification_cls = ConsoleNotification
    ai_client = AIClient(app.ai)
    catalog_client = CatalogClient(app.catalog)

    return (
        DependencyRegistration(OnyxConfig, config, is_instance=True),
        DependencyRegistration(Session, read_session_factory(engine)),
        DependencyRegistration(
            AbstractUnitOfWork, sql_uow_factory(session_factory=write_session_factory, cls=UnitOfWork)
        ),
        DependencyRegistration(AbstractFeedbackAnalytics, feedback_analytics_cls),
        DependencyRegistration(AbstractNotification, notification_cls),
        DependencyRegistration(AbstractAIClient, ai_client, is_instance=True),
        DependencyRegistration(AbstractCatalogClient, catalog_client, is_instance=True),
    )
