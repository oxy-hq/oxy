import abc
import os
import shlex
from pathlib import Path
from signal import SIGTERM
from typing import TypedDict, cast

import delegator
from onyx.catalog.adapters.pipeline.env_manager import AbstractPipelineEnvManager
from onyx.catalog.models.integration import Integration
from onyx.shared.logging import Logged
from onyx.shared.models.constants import EnvConfigType, IntegrationSlugChoices
from pexpect.popen_spawn import PopenSpawn


class AbstractPipelineClient(abc.ABC):
    @abc.abstractmethod
    def get_embed_command(self, integration: Integration) -> tuple[str, EnvConfigType]:
        ...

    @abc.abstractmethod
    def run_ingest_integration(self, integration: Integration) -> tuple[str, str]:
        ...

    @abc.abstractmethod
    def run_embed_integration(self, integration: Integration) -> tuple[str, str]:
        ...


class OAuthConfig(TypedDict):
    client_id: str
    client_secret: str


class MeltanoPipelineClient(Logged, AbstractPipelineClient):
    INGEST_COMMANDS = {
        IntegrationSlugChoices.salesforce: (
            "tap-salesforce",
            "ingest-salesforce",
        ),
        IntegrationSlugChoices.gmail: (
            "tap-gmail",
            "ingest-gmail",
        ),
        IntegrationSlugChoices.slack: (
            "tap-slack",
            "ingest-slack",
        ),
        IntegrationSlugChoices.notion: (
            "tap-notion",
            "ingest-notion",
        ),
        IntegrationSlugChoices.file: (
            "tap-file",
            "ingest-file",
        ),
    }
    EMBED_COMMANDS = {
        IntegrationSlugChoices.salesforce: (
            "tap-clickhouse",
            "embed-salesforce",
        ),
        IntegrationSlugChoices.gmail: (
            "tap-clickhouse",
            "embed-gmail",
        ),
        IntegrationSlugChoices.slack: (
            "tap-clickhouse",
            "embed-slack",
        ),
        IntegrationSlugChoices.notion: (
            "tap-clickhouse",
            "embed-notion",
        ),
        IntegrationSlugChoices.file: (
            "tap-clickhouse",
            "embed-file",
        ),
    }

    def __init__(
        self,
        meltano_project_root: str,
        env_manager: AbstractPipelineEnvManager,
        meltano_binary_path: str = ".meltano/run/bin",
        idle_timeout: int = 60 * 10,
        graceful_timeout: int = 30,
    ) -> None:
        self.__project_root = meltano_project_root
        self.__binary_path = meltano_binary_path
        self.__idle_timeout = idle_timeout
        self.__graceful_timeout = graceful_timeout
        self.__env_manager = env_manager

    @property
    def project_cwd(self):
        return Path(os.getcwd()) / self.__project_root

    def __run_command(self, sh_command: str, env: EnvConfigType):
        self.log.info(f"Executing shell command: {sh_command}")
        command = delegator.run(
            shlex.split(sh_command),
            cwd=self.project_cwd,
            env=env,
            block=False,
            timeout=self.__idle_timeout,
        )
        output = ""
        return_code = -1
        try:
            subprocess = cast(PopenSpawn, command.subprocess)
            for line in subprocess:
                output = line
                self.log.info("%s", line)
            return_code = subprocess.proc.wait(timeout=self.__graceful_timeout)
        except Exception as e:
            self.log.error("%s", e)
            subprocess.kill(SIGTERM)

        self.log.info("Command return code: %s", return_code)
        if return_code:
            raise RuntimeError(f"Exit code {return_code} for command {sh_command} with output {output}")
        return return_code, output

    def __execute(self, integration_id: str, catalog_tap: str, job_name: str, env: EnvConfigType):
        self.log.info(f"Executing job {job_name} with env {env}")
        catalog_path = f"extract/{integration_id}.{catalog_tap}.json"
        catalog_sh_command = f'/bin/bash -c "[[ ! -e {catalog_path} ]] && {self.__binary_path} invoke {catalog_tap} --discover > {catalog_path} || exit 0"'
        _, _ = self.__run_command(catalog_sh_command, env)

        ingest_sh_command = f'/bin/bash -c "{self.__binary_path} run --state-id-suffix={integration_id} {job_name}"'
        return_code, output = self.__run_command(
            ingest_sh_command,
            {
                **env,
                f"{catalog_tap.upper()}__CATALOG": catalog_path,
            },
        )
        return return_code, output

    def get_ingest_command(self, integration: Integration):
        catalog_tap, job_name = self.INGEST_COMMANDS[integration.slug]
        return catalog_tap, job_name, self.__env_manager.get_ingest_env(integration=integration)

    def get_embed_command(self, integration: Integration):
        catalog_tap, job_name = self.EMBED_COMMANDS[integration.slug]
        return catalog_tap, job_name, self.__env_manager.get_embed_env(integration=integration)

    def run_ingest_integration(self, integration: Integration):
        catalog_command, command, command_env = self.get_ingest_command(integration=integration)
        return self.__execute(str(integration.id), catalog_command, command, command_env)

    def run_embed_integration(self, integration: Integration):
        catalog_command, command, command_env = self.get_embed_command(integration=integration)
        return self.__execute(str(integration.id), catalog_command, command, command_env)
