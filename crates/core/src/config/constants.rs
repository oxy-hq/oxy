use std::time::Duration;

pub const EXECUTE_SQL_SOURCE: &str = "execute_sql";
pub const VALIDATE_SQL_SOURCE: &str = "validate_sql";
pub const RETRIEVAL_SOURCE: &str = "retrieval";
pub const TOOL_SOURCE: &str = "tool";
pub const AGENT_SOURCE: &str = "agent";
pub const AGENT_SOURCE_TYPE: &str = "agent_type";
pub const AGENT_SOURCE_PROMPT: &str = "prompt";
pub const AGENT_SOURCE_CONTENT: &str = "content";
pub const WORKFLOW_SOURCE: &str = "workflow";
pub const TASK_SOURCE: &str = "task";
pub const ARTIFACT_SOURCE: &str = "artifact";
pub const EVAL_SOURCE_ROOT: &str = "eval_root";
pub const EVAL_SOURCE: &str = "eval";
pub const EVAL_METRICS_POSTFIX: &str = "metrics";
pub const LOOP_VAR_NAME: &str = "value";
pub const CHECKPOINT_ROOT_PATH: &str = ".checkpoint";
pub const CHECKPOINT_DATA_PATH: &str = "checkpoint_data";
pub const DATABASE_SEMANTIC_PATH: &str = ".databases";
pub const SEMANTIC_MODEL_PATH: &str = "models";
pub const GLOBAL_SEMANTIC_PATH: &str = "semantics.yml";
pub const CHECKPOINT_EVENTS_FILE: &str = "events.jsonl";
pub const CHECKPOINT_SUCCESS_MARKER: &str = "SUCCESS";
pub const GEMINI_API_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1";
pub const CONCURRENCY_SOURCE: &str = "concurrency";
pub const CONCURRENCY_ITEM_ID_PREFIX: &str = "concurrency_item_";
pub const CACHE_SOURCE: &str = "cache";
pub const CONSISTENCY_SOURCE: &str = "consistency";
pub const CONSISTENCY_THRESHOLD: f32 = 0.25;
pub const MARKDOWN_MAX_FENCES: usize = 10;
pub const AGENT_RETRY_MAX_ELAPSED_TIME: Duration = Duration::from_secs(60 * 2);

pub const RETRIEVAL_INCLUSION_MIDPOINT_COLUMN: &str = "inclusion_midpoint";
pub const RETRIEVAL_EXCLUSION_BUFFER_MULTIPLIER: f32 = 0.9;
pub const RETRIEVAL_DEFAULT_INCLUSION_RADIUS: f32 = 0.2;
pub const VECTOR_INDEX_MIN_ROWS: usize = 1000;
pub const FTS_INDEX_MIN_ROWS: usize = 50;

pub const DEFAULT_API_KEY_HEADER: &str = "X-API-KEY";
pub const GCP_IAP_HEADER_KEY: &str = "x-goog-iap-jwt-assertion";
pub const GCP_IAP_SUB_HEADER_KEY: &str = "x-goog-authenticated-user-id";
pub const GCP_IAP_EMAIL_HEADER_KEY: &str = "x-goog-authenticated-user-email";
pub const GCP_IAP_ISS: &str = "https://cloud.google.com/iap";
pub const GCP_IAP_AUD_ENV_VAR: &str = "GCP_IAP_AUDIENCE";
pub const AUTHENTICATION_HEADER_KEY: &str = "authorization";
pub const AUTHENTICATION_SECRET_KEY: &str = "authentication_secret";
// The public keys used to verify Google Cloud IAP JWT tokens.
// https://www.gstatic.com/iap/verify/public_key-jwk
pub const GCP_IAP_PUBLIC_JWT_KEY: &str = r#"
{
  "keys": [
    {
      "alg": "ES256",
      "crv": "P-256",
      "kid": "LInRpg",
      "kty": "EC",
      "use": "sig",
      "x": "N2bcoT65Wk5NL6TzqSMObO9zyvgSC-XXYziCO9Wv5X4",
      "y": "Xce9Zw73BTQdCX0a36GGmCmSjALPpF8GJ9VJacJJq44"
    },
    {
      "alg": "ES256",
      "crv": "P-256",
      "kid": "4BCyVw",
      "kty": "EC",
      "use": "sig",
      "x": "OhtoYH87QMDZXjoSrOyCNAN-64exaO_G4VmZ2XbtSW8",
      "y": "KclJc4-WIBBYZ0SAhovElMqkPDvvZzRS3IfzGHpTnaE"
    },
    {
      "alg": "ES256",
      "crv": "P-256",
      "kid": "BD7LWA",
      "kty": "EC",
      "use": "sig",
      "x": "LrTwh8X_exLYYh8HOMuBr1eW5MKkE7S3s5WsyFkmO4k",
      "y": "uupeaTQD-c-CiSphTjohV6YoQFqYZ0wWtwrHjqvpIKs"
    },
    {
      "alg": "ES256",
      "crv": "P-256",
      "kid": "4w_puw",
      "kty": "EC",
      "use": "sig",
      "x": "kfvrQkED5kI4OYN6Pwu3v82MYx8ATlz7C5V6P4ag-q4",
      "y": "CGvUxkfGJCnjh41sWU7Kc4kp13yNWo4W4jCvZ67yP2w"
    },
    {
      "alg": "ES256",
      "crv": "P-256",
      "kid": "pYM-2A",
      "kty": "EC",
      "use": "sig",
      "x": "wvUvUqiVAO55EXHDfVxmIg5Y1yiz3Nw5wa4VOjL1oYQ",
      "y": "wK1eK3sQWmqmXtE95ItYZtHV2hT8Rpvqh8URDZnTNDQ"
    }
  ]
}"#;

pub const CONSISTENCY_PROMPT: &str = indoc::indoc! {"
    You are comparing a pair of submitted answers on a given question. Here is the data:
    [BEGIN DATA]
    ************
    [Question]: {{ task_description }}
    ************
    [Submission 1]: {{submission_1}}
    ************
    [Submission 2]: {{submission_2}}
    ************
    [END DATA]

    Compare the factual content of the submitted answers. Ignore any differences in style, grammar, punctuation. Answer the question by selecting one of the following options:
    A. The submitted answers are either a superset or contains each other and is fully consistent with it.
    B. There is a disagreement between the submitted answers.

    - First, highlight the disagreements between the two submissions.
    Following is the syntax to highlight the differences:

    (1) <factual_content>
    +++ <submission_1_factual_content_diff>
    --- <submission_2_factual_content_diff>

    [BEGIN EXAMPLE]
    Here are the key differences between the two submissions:
    (1) Capital of France
    +++ Paris
    --- France
    [END EXAMPLE]

    - Then reason about the highlighted differences. The submitted answers may either be a subset or superset of each other, or it may conflict. Determine which case applies.
    - At the end, print only a single choice from AB (without quotes or brackets or punctuation) on its own line corresponding to the correct answer. e.g A

    Reasoning:
"};
