from abc import ABC
from typing import TYPE_CHECKING

from onyx.catalog.adapters.repository import (
    AbstractCMSRepository,
    AbstractConnectionRepository,
    AbstractPromptRepository,
    AgentRepository,
    AgentVersionRepository,
    CMSRepository,
    ConnectionRepository,
    IntegrationRepository,
    NamespaceRepository,
    PromptRepository,
    TaskRepository,
    UserAgentLikeRepository,
)
from onyx.shared.adapters.orm.mixins import SQLUnitOfWorkMixin
from onyx.shared.services.unit_of_work import (
    AbstractUnitOfWork as _AbstractUnitOfWork,
)
from sqlalchemy.orm import Session

if TYPE_CHECKING:
    from onyx.catalog.adapters.repository import (
        AbstractAgentRepository,
        AbstractAgentVersionRepository,
        AbstractIntegrationRepository,
        AbstractNamespaceRepository,
        AbstractTaskRepository,
        AbstractUserAgentLikeRepository,
    )


class AbstractUnitOfWork(_AbstractUnitOfWork, ABC):
    integrations: "AbstractIntegrationRepository"
    agents: "AbstractAgentRepository"
    agent_versions: "AbstractAgentVersionRepository"
    namespaces: "AbstractNamespaceRepository"
    tasks: "AbstractTaskRepository"
    prompts: "AbstractPromptRepository"
    user_agent_like: "AbstractUserAgentLikeRepository"
    cms: "AbstractCMSRepository"
    connections: "AbstractConnectionRepository"


class UnitOfWork(SQLUnitOfWorkMixin, AbstractUnitOfWork):
    def _init_repositories(self, session: Session):
        self.integrations = IntegrationRepository(session)
        self.namespaces = NamespaceRepository(session)
        self.agents = AgentRepository(session)
        self.prompts = PromptRepository(session)
        self.tasks = TaskRepository(session)
        self.agent_versions = AgentVersionRepository(session)
        self.user_agent_like = UserAgentLikeRepository(session)
        self.cms = CMSRepository(session)
        self.connections = ConnectionRepository(session)

    @property
    def session(self):
        return self._session
