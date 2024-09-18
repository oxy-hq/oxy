import abc
import json
import shlex

import delegator
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged


class AbstractSecretsManager(Logged, abc.ABC):
    @abc.abstractmethod
    def encrypt(self, plaintext: str) -> str:
        ...

    @abc.abstractmethod
    def decrypt(self, ciphertext: str) -> str:
        ...

    def encrypt_dict(self, config: dict[str, str] | str) -> dict[str, str]:
        if not isinstance(config, str):
            config = json.dumps(config)

        return json.loads(self.encrypt(config), strict=False)

    def decrypt_dict(self, config: dict[str, str]) -> dict[str, str]:
        decrypted = self.decrypt(json.dumps(config))
        return json.loads(decrypted, strict=False)


class EncryptionError(Exception):
    ...


class SOPSSecretsManager(AbstractSecretsManager):
    def __init__(self, config: OnyxConfig) -> None:
        self.__key_id = config.sops.key_id
        self.__vendor = config.sops.vendor

    @property
    def __params(self) -> str:
        return f"--{self.__vendor.value}={self.__key_id}"

    def encrypt(self, plaintext: str) -> str:
        result = delegator.run(f"echo {shlex.quote(plaintext)} | sops -e {self.__params} /dev/stdin")
        if result.err:
            raise EncryptionError(result.err)
        return result.out

    def decrypt(self, ciphertext: str) -> str:
        return delegator.run(
            f"echo {shlex.quote(ciphertext.encode('unicode_escape').decode())} | sops -d {self.__params} /dev/stdin"
        ).out
