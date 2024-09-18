import abc
import json

import requests
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from requests.auth import HTTPBasicAuth
from typing_extensions import TypedDict


class NotionUserInfo(TypedDict):
    name: str
    email: str


class AbstractNotion(abc.ABC):
    @abc.abstractmethod
    def get_access_token(self, code: str) -> str:
        ...

    @abc.abstractmethod
    def get_user_info(self, token: str) -> NotionUserInfo:
        ...


class NotionAPIError(Exception):
    pass


class Notion(Logged, AbstractNotion):
    def __init__(self, config: OnyxConfig) -> None:
        integration_config = config.integration
        self.client_id = integration_config.notion_client_id
        self.client_secret = integration_config.notion_client_secret
        self.api_url = integration_config.notion_api_url
        self.redirect_url = integration_config.notion_redirect_url

    def _handle_api_error(self, response, error_message):
        log_message = f"{error_message} Status code: {response.status_code}, Response: {response.text}"
        self.log.error(log_message)
        raise NotionAPIError(log_message)

    def get_access_token(self, code: str):
        headers = {
            "Content-Type": "application/json",
        }

        payload = {
            "code": code,
            "redirect_uri": self.redirect_url,
            "grant_type": "authorization_code",
        }

        response = requests.post(
            f"{self.api_url}/oauth/token",
            auth=HTTPBasicAuth(self.client_id, self.client_secret),
            headers=headers,
            data=json.dumps(payload),
        )

        if response.status_code == 200:
            data = response.json()
            access_token = data.get("access_token")

            return access_token
        else:
            self._handle_api_error(response, "Error obtaining access token.")

    def get_user_info(self, token: str):
        headers = {"Authorization": f"Bearer {token}", "Notion-Version": "2022-06-28"}
        response = requests.get(f"{self.api_url}/users/me", headers=headers)

        if response.status_code == 200:
            user_info = response.json()
            return {
                "name": user_info.get("bot", {}).get("workspace_name", "Notion's default workspace"),
                "email": user_info.get("bot")["owner"]["user"]["person"]["email"],
            }
        else:
            self._handle_api_error(response, "Error obtaining user information.")
