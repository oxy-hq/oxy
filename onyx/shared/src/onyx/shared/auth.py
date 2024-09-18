import abc
from dataclasses import dataclass

import jwt
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged


@dataclass
class AuthMetadata:
    user_id: str
    organization_id: str
    role: str


class AuthenticationError(Exception):
    pass


class AbstractAuthAdapter(abc.ABC):
    @property
    @abc.abstractmethod
    def auth_metadata_key(self) -> str:
        ...

    @abc.abstractmethod
    def verify(self, token: str) -> AuthMetadata:
        ...


def jwt_verify(token: str, secret: str) -> AuthMetadata:
    try:
        decoded_values = jwt.decode(
            # Supabase auth token includes audience
            token,
            secret,
            audience="authenticated",
            algorithms=["HS256"],
        )

        return AuthMetadata(
            user_id=decoded_values["sub"],
            # TODO: will apply when organization and role have implemented
            organization_id="",
            role="",
        )
    except jwt.PyJWTError as exc:
        raise AuthenticationError("Invalid token") from exc


class JWTAuthAdapter(Logged, AbstractAuthAdapter):
    def __init__(self, config: OnyxConfig):
        self._secret = config.grpc.auth_secret
        self._key = config.grpc.auth_metadata_key

    @property
    def auth_metadata_key(self) -> str:
        return self._key

    def verify(self, token: str) -> AuthMetadata:
        return jwt_verify(token, self._secret)
