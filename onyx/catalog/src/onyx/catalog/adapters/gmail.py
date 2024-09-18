import abc
from typing import Tuple

import requests
from onyx.shared.config import OnyxConfig
from onyx.shared.logging import Logged
from typing_extensions import TypedDict


class GmailUserInfo(TypedDict):
    email: str


class AbstractGmail(abc.ABC):
    @abc.abstractmethod
    def get_refresh_token(self, code: str) -> Tuple[str, str]:
        ...

    @abc.abstractmethod
    def get_user_info(self, token: str) -> GmailUserInfo:
        ...


class GmailAPIError(Exception):
    pass


class Gmail(Logged, AbstractGmail):
    def __init__(self, config: OnyxConfig) -> None:
        integration_config = config.integration
        self.client_id = integration_config.gmail_client_id
        self.client_secret = integration_config.gmail_client_secret
        self.oauth2_url = integration_config.gmail_oauth2_url
        self.openid_url = integration_config.gmail_openid_url
        self.redirect_url = integration_config.gmail_redirect_url

    def _handle_api_error(self, response, error_message):
        log_message = f"{error_message} Status code: {response.status_code}, Response: {response.text}"
        self.log.error(log_message)
        raise GmailAPIError(log_message)

    def get_refresh_token(self, code: str):
        payload = {
            "code": code,
            "redirect_uri": self.redirect_url,
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "grant_type": "authorization_code",
        }

        response = requests.post(f"{self.oauth2_url}/token", data=payload)

        if response.status_code == 200:
            data = response.json()
            refresh_token = data.get("refresh_token")
            access_token = data.get("access_token")

            return refresh_token, access_token
        else:
            self._handle_api_error(response, "Error obtaining refresh token.")

    def get_user_info(self, token: str):
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.get(f"{self.openid_url}/userinfo", headers=headers)

        if response.status_code == 200:
            user_info = response.json()
            return user_info
        else:
            self._handle_api_error(response, "Error obtaining user information.")
