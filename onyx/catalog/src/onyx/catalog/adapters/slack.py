import abc
import logging
from typing import TypedDict

import requests
from onyx.shared.config import OnyxConfig


class SlackUserInfo(TypedDict):
    team_id: str
    team_name: str
    access_token: str


class AbstractSlack(abc.ABC):
    @abc.abstractmethod
    def get_oauth_access(self, code: str) -> SlackUserInfo:
        ...


class SlackAPIError(Exception):
    pass


logger = logging.getLogger(__name__)


class Slack(AbstractSlack):
    def __init__(self, config: OnyxConfig) -> None:
        integration_config = config.integration
        self.client_id = integration_config.slack_client_id
        self.client_secret = integration_config.slack_client_secret
        self.oauth2_url = integration_config.slack_oauth2_url
        self.redirect_uri = integration_config.slack_redirect_url

    def get_oauth_access(self, code: str):
        payload = {
            "code": code,
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "redirect_uri": self.redirect_uri,
        }

        response = requests.post(f"{self.oauth2_url}/oauth.v2.access", data=payload)

        if response.status_code == 200:
            data = response.json()
            ok = data.get("ok")
            if not ok:
                error = data.get("error")
                logger.error(f"Error obtaining access oauth: {error}")
                raise SlackAPIError(f"Error obtaining access oauth, error: {error}")

            team = data.get("team")
            team_id = team.get("id")
            team_name = team.get("name")
            access_token = data.get("access_token")

            return {
                "team_id": team_id,
                "team_name": team_name,
                "access_token": access_token,
            }
        else:
            logger.error(f"Error obtaining access oauth: {response.text}")
            raise SlackAPIError(
                f"Error obtaining access oauth. Status code: {response.status_code}, Response: {response.text}"
            )
