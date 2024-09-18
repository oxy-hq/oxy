import abc
import uuid

from onyx.catalog.models.agent import Agent
from onyx.catalog.models.agent_version import AgentVersion
from onyx.catalog.models.cms.agent_category import AgentCategory
from onyx.catalog.models.cms.agent_featured import AgentFeatured
from onyx.catalog.models.cms.tabs import DiscoverTab
from onyx.catalog.models.connection import Connection
from onyx.catalog.models.ingest_state import IngestState
from onyx.catalog.models.integration import Integration
from onyx.catalog.models.namespace import Namespace
from onyx.catalog.models.prompt import Prompt
from onyx.catalog.models.task import Task
from onyx.catalog.models.user_agent_like import UserAgentLike
from onyx.shared.adapters.orm.repository import GenericSqlRepository
from onyx.shared.adapters.repository import GenericRepository
from onyx.shared.logging import Logged
from onyx.shared.models.constants import (
    DEFAULT_NAMESPACE,
)
from sqlalchemy import select
from sqlalchemy.orm import Session, selectinload
from sqlalchemy.sql.expression import false


class AbstractAgentRepository(GenericRepository[Agent], abc.ABC):
    @abc.abstractmethod
    def list_by_subdomains(self, subdomains: list[str]) -> list[Agent]:
        ...


class AbstractCMSRepository(GenericRepository[DiscoverTab], abc.ABC):
    @abc.abstractmethod
    def update_featured(self, new_featured: list[AgentFeatured]) -> list[AgentFeatured]:
        ...

    @abc.abstractmethod
    def update_tabs(self, new_tabs: list[DiscoverTab]) -> list[DiscoverTab]:
        ...

    @abc.abstractmethod
    def get_categories(self, categories: list[str]) -> list[AgentCategory]:
        ...


class AbstractAgentVersionRepository(GenericRepository[AgentVersion], abc.ABC):
    ...


class AbstractIntegrationRepository(GenericRepository[Integration], abc.ABC):
    @abc.abstractmethod
    def get_latest_task(self, id: uuid.UUID) -> Task | None:
        ...

    @abc.abstractmethod
    def list_by_ids(self, ids: list[uuid.UUID]) -> list[Integration]:
        ...

    @abc.abstractmethod
    def get_or_create_ingest_state(self, id: uuid.UUID) -> IngestState:
        ...

    @abc.abstractmethod
    def get_ingest_state_for_update(self, id: uuid.UUID) -> IngestState:
        ...


class AbstractPromptRepository(GenericRepository[Prompt], abc.ABC):
    ...


class AbstractNamespaceRepository(GenericRepository[Namespace], abc.ABC):
    @abc.abstractmethod
    def get_default_namespace(self, organization_id: uuid.UUID) -> Namespace:
        ...

    @abc.abstractmethod
    def get_private_namespace(self, organization_id: uuid.UUID, user_id: uuid.UUID) -> Namespace:
        ...

    @abc.abstractmethod
    def delete_integration(self, id: str):
        ...

    @property
    def default_namespace_name(self):
        return DEFAULT_NAMESPACE

    def generate_private_namespace_name(self, organization_id: uuid.UUID, user_id: uuid.UUID):
        return f"o_{organization_id}/u_{user_id}"


class AbstractTaskRepository(GenericRepository[Task], abc.ABC):
    ...


class IntegrationRepository(GenericSqlRepository[Integration], AbstractIntegrationRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Integration)

    def get_latest_task(self, integration_id: uuid.UUID):
        found = (
            self._session.execute(
                select(Task)
                .where(
                    Task.source_id == integration_id,
                )
                .order_by(Task.created_at.desc())
            )
            .scalars()
            .first()
        )
        return found

    def list_by_ids(self, ids: list[uuid.UUID]):
        stmt = select(Integration).where(Integration.id.in_(ids))
        results = self._session.execute(stmt).scalars().all()
        return list(results)

    def get_or_create_ingest_state(self, id: uuid.UUID):
        stmt = select(IngestState).where(IngestState.integration_id == id)
        state = self._session.execute(stmt).scalar_one_or_none()
        if not state:
            state = IngestState(integration_id=id)
            self._session.add(state)
            self._session.commit()
        return state

    def get_ingest_state_for_update(self, id: uuid.UUID):
        stmt = select(IngestState).where(IngestState.integration_id == id).with_for_update()
        state = self._session.execute(stmt).scalar_one_or_none()
        return state


class NamespaceRepository(GenericSqlRepository[Namespace], Logged, AbstractNamespaceRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Namespace)

    def get_default_namespace(self, organization_id: uuid.UUID) -> Namespace:
        found = self._session.execute(
            select(Namespace).where(
                Namespace.organization_id == organization_id,
                Namespace.owner_id == organization_id,
            )
        ).scalar_one_or_none()
        if found:
            return found
        return self.add(
            Namespace(
                name=self.default_namespace_name,
                organization_id=organization_id,
                owner_id=organization_id,
            )
        )

    def get_private_namespace(self, organization_id: uuid.UUID, user_id: uuid.UUID):
        found = self._session.execute(
            select(Namespace).where(
                Namespace.organization_id == organization_id,
                Namespace.owner_id == user_id,
            )
        ).scalar_one_or_none()
        if found:
            return found
        return self.add(
            Namespace(
                name=self.generate_private_namespace_name(organization_id=organization_id, user_id=user_id),
                organization_id=organization_id,
                owner_id=user_id,
            )
        )

    def get_namespaces(
        self,
        organization_id: uuid.UUID,
        user_id: uuid.UUID,
    ):
        org_ns = self.get_default_namespace(organization_id).id
        user_ns = self.get_private_namespace(organization_id, user_id).id
        self.log.info(f"org_ns: {org_ns}, user_ns: {user_ns}")
        return [user_ns, org_ns]

    def delete_integration(self, id: str):
        integration = self._session.query(Integration).get(id)
        if integration:
            self._session.delete(integration)
            self._session.commit()


class AgentVersionRepository(GenericSqlRepository[AgentVersion], AbstractAgentVersionRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, AgentVersion)


class AgentRepository(GenericSqlRepository[Agent], AbstractAgentRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Agent)

    def get_by_id(self, id: uuid.UUID) -> Agent | None:
        agent = self._session.execute(
            select(Agent)
            .where(Agent.id == id)
            .where(Agent.is_deleted == false())
            .options(selectinload(Agent.published_version), selectinload(Agent.dev_version))
        ).scalar_one_or_none()
        return agent

    def list_by_subdomains(self, subdomains: list[str]) -> list[Agent]:
        stmt = select(Agent).where(Agent.published_version.has(AgentVersion.subdomain.in_(subdomains)))
        results = self._session.execute(stmt).scalars().all()
        return list(results)


class CMSRepository(GenericSqlRepository[DiscoverTab], AbstractCMSRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, DiscoverTab)

    def update_featured(self, new_featured: list[AgentFeatured]) -> list[AgentFeatured]:
        stmt = select(AgentFeatured)
        old_featured = self._session.scalars(stmt).all()
        for featured in old_featured:
            self._session.delete(featured)
        self._session.add_all(new_featured)
        return new_featured

    def update_tabs(self, new_tabs: list[DiscoverTab]) -> list[DiscoverTab]:
        stmt = select(DiscoverTab)
        results = self._session.scalars(stmt).all()
        for tab in results:
            self._session.delete(tab)
        self._session.add_all(new_tabs)
        return new_tabs

    def get_categories(self, categories: list[str]) -> list[AgentCategory]:
        agent_categories = []
        for category in categories:
            agent_category = self.get_or_create_category(category)
            agent_categories.append(agent_category)
        return agent_categories

    def get_or_create_category(self, category: str):
        existing = self._session.scalars(select(AgentCategory).where(AgentCategory.value == category)).one_or_none()
        if existing:
            return existing
        new_category = AgentCategory(label=category, value=category)
        self._session.add(new_category)
        self._session.flush()
        return new_category


class TaskRepository(GenericSqlRepository[Task], AbstractTaskRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Task)


class AbstractUserAgentLikeRepository(GenericRepository[UserAgentLike], abc.ABC):
    @abc.abstractmethod
    def create_user_agent_like(self, user_id: uuid.UUID, agent_id: uuid.UUID):
        ...

    @abc.abstractmethod
    def delete_user_agent_like(self, user_id: uuid.UUID, agent_id: uuid.UUID):
        ...


class UserAgentLikeRepository(GenericSqlRepository[UserAgentLike], Logged, AbstractUserAgentLikeRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, UserAgentLike)

    def create_user_agent_like(self, user_id: uuid.UUID, agent_id: uuid.UUID):
        try:
            user_agent_like = UserAgentLike(user_id=user_id, agent_id=agent_id)

            self._session.add(user_agent_like)
            self._session.commit()

            return True
        except Exception as error:
            self.log.error(f"error creating user agent like: {error}")
            return False

    def delete_user_agent_like(self, user_id: uuid.UUID, agent_id: uuid.UUID):
        try:
            user_agent_like = self._session.execute(
                select(UserAgentLike).where(UserAgentLike.user_id == user_id).where(UserAgentLike.agent_id == agent_id)
            ).scalar_one_or_none()

            if user_agent_like:
                self._session.delete(user_agent_like)
                self._session.commit()
                return True
        except Exception as error:
            self.log.error(f"error deleting user agent like: {error}")
            return False


class PromptRepository(GenericSqlRepository[Prompt], AbstractPromptRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Prompt)


class AbstractConnectionRepository(GenericRepository[Connection], abc.ABC):
    @abc.abstractmethod
    def list_by_ids(self, ids: list[uuid.UUID]) -> list[Connection]:
        ...


class ConnectionRepository(GenericSqlRepository[Connection], AbstractConnectionRepository):
    def __init__(self, session: Session) -> None:
        super().__init__(session, Connection)

    def list_by_ids(self, ids: list[uuid.UUID]):
        stmt = select(Connection).where(Connection.id.in_(ids))
        results = self._session.scalars(stmt).all()
        return list(results)
