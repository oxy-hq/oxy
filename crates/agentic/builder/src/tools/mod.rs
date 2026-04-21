//! Built-in tools for the builder copilot.
//!
//! - `search_files`      — glob pattern search across the project directory
//! - `read_file`         — read file content (optionally by line range)
//! - `search_text`       — grep-like text search across the codebase
//! - `propose_change`    — propose a file change, suspend for user confirmation
//! - `validate_project`  — validate project files against schema
//! - `run_tests`         — run one or more `.test.yml` files
//! - `execute_sql`       — run a SQL query against a configured database
//! - `semantic_query`    — compile and execute a semantic layer query
//!
//! All file-system operations are sandboxed to the project root.

mod execute_sql;
mod lookup_schema;
mod propose_change;
mod read_file;
mod run_tests;
mod search_files;
mod search_text;
mod semantic_query;
mod utils;
mod validate_project;

use agentic_core::tools::ToolDef;

use crate::schema_provider::BuilderSchemaProvider;

pub use execute_sql::execute_execute_sql;
pub use lookup_schema::execute_lookup_schema;
pub use propose_change::{
    ChangeBlock, apply_blocks_to_content, apply_change_blocks, delete_file, execute_propose_change,
    write_file_content,
};
pub use read_file::execute_read_file;
pub use run_tests::execute_run_tests;
pub use search_files::execute_search_files;
pub use search_text::execute_search_text;
pub use semantic_query::execute_semantic_query;
pub use utils::safe_path;
pub use validate_project::execute_validate_project;

/// All tools available to the builder copilot.
pub fn all_tools(schema_provider: &dyn BuilderSchemaProvider) -> Vec<ToolDef> {
    vec![
        search_files::search_files_def(),
        read_file::read_file_def(),
        search_text::search_text_def(),
        propose_change::propose_change_def(),
        validate_project::validate_project_def(),
        lookup_schema::lookup_schema_def(schema_provider),
        run_tests::run_tests_def(),
        execute_sql::execute_sql_def(),
        semantic_query::semantic_query_def(),
        agentic_core::tools::ask_user_tool_def(),
    ]
}
