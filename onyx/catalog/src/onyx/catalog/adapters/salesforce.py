import abc

import requests
from onyx.shared.config import OnyxConfig
from typing_extensions import TypedDict


class SalesforceUserInfo(TypedDict):
    email: str


class AbstractSalesforce(abc.ABC):
    @abc.abstractmethod
    def get_refresh_token(self, code: str) -> str:
        ...

    @abc.abstractmethod
    def get_user_info(self, token: str) -> SalesforceUserInfo:
        ...


class SalesforceAPIError(Exception):
    pass


class Salesforce(AbstractSalesforce):
    def __init__(
        self,
        config: OnyxConfig,
    ) -> None:
        integration_config = config.integration
        self.client_id = integration_config.salesforce_client_id
        self.client_secret = integration_config.salesforce_client_secret
        self.oauth2_url = integration_config.salesforce_oauth2_url
        self.redirect_url = integration_config.salesforce_redirect_url

    def get_refresh_token(self, code: str):
        payload = {
            "code": code,
            "grant_type": "authorization_code",
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "redirect_uri": self.redirect_url,
        }

        response = requests.post(f"{self.oauth2_url}/token", data=payload)

        if response.status_code == 200:
            data = response.json()
            refresh_token = data.get("refresh_token")
            access_token = data.get("access_token")

            return refresh_token, access_token
        else:
            print("Error obtaining refresh token:", response.text)
            raise SalesforceAPIError(
                f"Error obtaining refresh token. Status code: {response.status_code}, Response: {response.text}"
            )

    def get_user_info(self, token: str):
        headers = {"Authorization": f"Bearer {token}"}
        response = requests.get(f"{self.oauth2_url}/userinfo", headers=headers)

        if response.status_code == 200:
            user_info = response.json()
            return user_info
        else:
            raise SalesforceAPIError(
                f"Error obtaining user information. Status code: {response.status_code}, Response: {response.text}"
            )
