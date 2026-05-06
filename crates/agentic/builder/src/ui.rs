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
pub fn builder_tool_summary(tool: &str, input: &serde_json::Value) -> Option<String> {
    // Tools that ask the LLM to supply a human-readable `description` field —
    // use it directly so the summary reflects exactly what the LLM intends to do.
    if matches!(tool, "write_file" | "edit_file" | "delete_file") {
        if let Some(desc) = input["description"].as_str().filter(|s| !s.is_empty()) {
            return Some(desc.to_string());
        }
    }

    let s = match tool {
        "search_files" => "Searching files",
        "read_file" => "Reading a file",
        "search_text" => "Searching file contents",
        "file_change" => "Making a file change",
        "write_file" => "Writing a file",
        "edit_file" => "Editing a file",
        "delete_file" => "Deleting a file",
        "manage_directory" => "Managing a directory",
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
