//! UI helpers for the builder domain.

/// Map a lower-cased builder pipeline state name to a user-friendly step summary.
pub fn builder_step_summary(state: &str) -> Option<String> {
    let s = match state {
        "clarifying" => "Preparing the builder request",
        "specifying" => "Structuring the builder task",
        "solving" => "Working",
        "executing" => "Applying output",
        "interpreting" => "Summarizing",
        "diagnosing" => "Recovering from an error",
        _ => return None,
    };
    Some(s.to_string())
}

/// Map a builder tool name to a user-friendly step summary.
pub fn builder_tool_summary(tool: &str) -> Option<String> {
    let s = match tool {
        "search_files" => "Searching files",
        "read_file" => "Reading a file",
        "search_text" => "Searching file contents",
        "propose_change" => "Making a file change",
        "validate_project" => "Validating objects",
        "lookup_schema" => "Looking up schema details",
        "run_tests" => "Running tests",
        "execute_sql" => "Running SQL",
        "semantic_query" => "Running a semantic query",
        "ask_user" => "Asking for user input",
        _ => return None,
    };
    Some(s.to_string())
}
