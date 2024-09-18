from onyx.catalog.adapters.connector.base import AbstractConnector
from onyx.catalog.models.errors import ConnectorNotSupported
from onyx.shared.models.constants import ConnectionSlugChoices

ConnectorMappingType = dict[ConnectionSlugChoices, type[AbstractConnector]]
ConnectorMappingTuple = tuple[ConnectionSlugChoices, type[AbstractConnector]]


class ConnectorRegistry:
    def __init__(self, *connector_mappings: ConnectorMappingTuple) -> None:
        self.__connector_mappings: dict[ConnectionSlugChoices, type[AbstractConnector]] = dict(connector_mappings)

    def register(self, slug: ConnectionSlugChoices, connector_cls: type[AbstractConnector]) -> None:
        self.__connector_mappings[slug] = connector_cls

    def unregister_all(self) -> None:
        self.__connector_mappings = {}

    def get_connector_cls(self, slug: ConnectionSlugChoices) -> type[AbstractConnector]:
        connector_cls = self.__connector_mappings.get(slug)
        if not connector_cls:
            raise ConnectorNotSupported(f"Connector for slug {slug} not found")
        return connector_cls
