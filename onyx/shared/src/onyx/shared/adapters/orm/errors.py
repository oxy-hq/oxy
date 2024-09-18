class ORMError(Exception):
    pass


class ORMInvalidColumnError(ORMError):
    pass


class RowLockedError(ORMError):
    pass
