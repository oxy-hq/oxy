import os

from onyx.shared.constants import KMSVendors
from onyx.shared.stage import Stage
from pydantic_settings import BaseSettings, SettingsConfigDict


class GRPCSettings(BaseSettings):
    server_host: str = "[::]"
    server_port: int = 50051
    metrics_port: int = 50052
    auth_metadata_key: str = "x-auth-token"
    auth_secret: str = "secret"
    reflection: bool = True


class DatabaseSettings(BaseSettings):
    connection_string: str = ""
    pool_size: int = 1
    pool_max_overflow: int = -1


class ClickhouseSettings(BaseSettings):
    host: str = "127.0.0.1"
    port: int = 8123
    username: str = "default"
    password: str = ""
    database: str = "default"
    secure: bool = False
    protocol: str = "clickhouse+http"


class VespaSettings(BaseSettings):
    application_name: str = "onyx"
    url: str = "http://localhost:8088"
    embedding_size: int = 1536  # text-embedding-ada-002
    deployment_type: str = "docker"  # "cloud" or "docker"
    agent_document_type: str = "agent"

    # docker settings
    container_name: str = "vespa"

    # cloud settings, must be available at BUILD TIME
    cloud_secret_token: str = ""
    cloud_tenant_name: str = "example"
    cloud_api_key: str = "fake"


class SOpsSettings(BaseSettings):
    key_id: str = "699D0E89C0C9EF9982FB6D33B44E7ABBE0E0BB0D"
    vendor: KMSVendors = KMSVendors.pgp


class MeltanoSettings(BaseSettings):
    project_root: str = f"{os.getcwd()}/meltano/pipeline"
    binary_path: str = ".meltano/run/bin"


class AirflowSettings(BaseSettings):
    web_server_url: str = "http://airflow-webserver:8080"
    user_name: str = "airflow"
    password: str = "airflow"


class TemporalSettings(BaseSettings):
    enabled: bool = True
    url: str = "localhost:7233"
    tls: bool = False
    api_key: str = ""
    namespace: str = "default"
    catalog_queue: str = "catalog_queue"


class IntegrationSettings(BaseSettings):
    salesforce_client_id: str = ""
    salesforce_client_secret: str = ""
    salesforce_oauth2_url: str = "https://login.salesforce.com/services/oauth2"
    salesforce_redirect_url: str = "https://localhost:3000/api/integration/salesforce/callback"
    # For more information on the google's oauth2 config endpoints see:
    # https://accounts.google.com/.well-known/openid-configuration
    gmail_client_id: str = ""
    gmail_client_secret: str = ""
    gmail_redirect_url: str = "http://localhost:3000/api/integration/gmail/callback"
    gmail_oauth2_url: str = "https://oauth2.googleapis.com"
    gmail_openid_url: str = "https://openidconnect.googleapis.com/v1"

    slack_client_id: str = ""
    slack_client_secret: str = ""
    slack_oauth2_url: str = "https://slack.com/api"
    slack_redirect_url: str = "https://localhost:3001/api/integration/slack/callback"

    notion_client_id: str = ""
    notion_client_secret: str = ""
    notion_redirect_url: str = "http://localhost:3000/api/integration/notion/callback"
    notion_api_url: str = "https://api.notion.com/v1"


class OpenAISettings(BaseSettings):
    base_url: str = ""
    api_key: str = ""
    organization: str = ""
    chat_model: str = "gpt-4o"
    embeddings_model: str = "text-embedding-ada-002"
    embeddings_max_tokens: int = 1000


class SentrySettings(BaseSettings):
    dsn: str = ""


class LangfuseSettings(BaseSettings):
    enabled: bool = False
    public_key: str = ""
    secret_key: str = ""
    host: str = "https://us.cloud.langfuse.com"


class SlackConfig(BaseSettings):
    bot_token: str = ""
    qa_log_channel: str = "test-channel-1"
    search_log_channel: str = "test-channel-1"


class GoogleSearchSettings(BaseSettings):
    serper_api_key: str = ""


class S3Settings(BaseSettings):
    endpoint: str = "http://minio:9000"
    region: str = str(os.environ.get("AWS_DEFAULT_REGION"))
    access_key_id: str = ""
    secret_access_key: str = ""
    bucket_name: str = "onyx-dev-us-west-2-workload-widely-heroic-gobbler"
    use_ssl: bool = False


class DSPYSettings(BaseSettings):
    predict_module_path: str = "backend-workspaces/ai/src/onyx/ai/dspy/agent.predict.json"


class SearchSettings(BaseSettings):
    ranking_threshold: float = 0


class OnyxConfig(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", env_nested_delimiter="__")
    grpc: GRPCSettings = GRPCSettings()
    database: DatabaseSettings = DatabaseSettings()
    clickhouse: ClickhouseSettings = ClickhouseSettings()
    vespa: VespaSettings = VespaSettings()
    sops: SOpsSettings = SOpsSettings()
    meltano: MeltanoSettings = MeltanoSettings()
    integration: IntegrationSettings = IntegrationSettings()
    openai: OpenAISettings = OpenAISettings()
    sentry: SentrySettings = SentrySettings()
    langfuse: LangfuseSettings = LangfuseSettings()
    slack: SlackConfig = SlackConfig()
    airflow: AirflowSettings = AirflowSettings()
    temporal: TemporalSettings = TemporalSettings()
    stage: Stage = Stage.local
    google_search: GoogleSearchSettings = GoogleSearchSettings()
    s3: S3Settings = S3Settings()
    dspy: DSPYSettings = DSPYSettings()
    search: SearchSettings = SearchSettings()
