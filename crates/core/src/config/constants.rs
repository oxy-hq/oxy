pub const EXECUTE_SQL_SOURCE: &str = "execute_sql";
pub const VALIDATE_SQL_SOURCE: &str = "validate_sql";
pub const RETRIEVAL_SOURCE: &str = "retrieval";
pub const TOOL_SOURCE: &str = "tool";
pub const AGENT_SOURCE: &str = "agent";
pub const AGENT_SOURCE_TYPE: &str = "agent_type";
pub const AGENT_SOURCE_PROMPT: &str = "prompt";
pub const AGENT_SOURCE_CONTENT: &str = "content";
pub const WORKFLOW_SOURCE: &str = "workflow";
pub const EVAL_SOURCE_ROOT: &str = "eval_root";
pub const EVAL_SOURCE: &str = "eval";
pub const EVAL_METRICS_POSTFIX: &str = "metrics";
pub const LOOP_VAR_NAME: &str = "value";
pub const CHECKPOINT_ROOT_PATH: &str = ".checkpoint";
pub const CHECKPOINT_DATA_PATH: &str = "checkpoint_data";
pub const DATABASE_SEMANTIC_PATH: &str = ".databases";
pub const SEMANTIC_MODEL_PATH: &str = "models";
pub const CHECKPOINT_EVENTS_FILE: &str = "events.jsonl";
pub const CHECKPOINT_SUCCESS_MARKER: &str = "SUCCESS";
pub const GEMINI_API_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1";
pub const CONCURRENCY_SOURCE: &str = "concurrency";
pub const CONCURRENCY_ITEM_ID_PREFIX: &str = "concurrency_item_";
pub const CACHE_SOURCE: &str = "cache";
pub const CONSISTENCY_SOURCE: &str = "consistency";
pub const CONSISTENCY_THRESHOLD: f32 = 0.25;

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
