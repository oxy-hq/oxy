class CatalogError(Exception):
    pass


class IntegrationNotFound(CatalogError):
    pass


class IntegrationAreBeingSynced(CatalogError):
    pass


class FailedToConnect(CatalogError):
    pass


class ConnectorNotSupported(CatalogError):
    pass


class ConnectionNotFound(CatalogError):
    pass


class ConnectionAreBeingSynced(CatalogError):
    pass


class SourceNotSupported(CatalogError):
    pass
