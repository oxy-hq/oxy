use std::env;
use tracing::info;

pub fn init_sentry() -> Option<sentry::ClientInitGuard> {
    let dsn = env::var("SENTRY_DSN").ok();
    if dsn.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        info!("Sentry DSN not found or empty in environment. Sentry will not be initialized.");
        return None;
    }

    let environment = if cfg!(debug_assertions) {
        "development".to_string()
    } else {
        env::var("SENTRY_ENVIRONMENT")
            .or_else(|_| env::var("ENVIRONMENT"))
            .or_else(|_| env::var("ENV"))
            .unwrap_or_else(|_| "production".to_string())
    };

    let release = format!("oxy@{}", env!("CARGO_PKG_VERSION"));

    let traces_sample_rate = if cfg!(debug_assertions) {
        0.0 // always disable
    } else {
        env::var("SENTRY_TRACES_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0) // always disable for now
    };

    let options = sentry::ClientOptions {
        dsn: dsn?.parse().ok(),
        environment: Some(environment.clone().into()),
        release: Some(release.clone().into()),
        traces_sample_rate,
        attach_stacktrace: true,
        send_default_pii: false, // Don't send personally identifiable information
        max_breadcrumbs: 100,
        before_send: Some(std::sync::Arc::new(|mut event| {
            // Filter out sensitive information
            if let Some(exception) = event.exception.iter_mut().next() {
                if let Some(stacktrace) = &mut exception.stacktrace {
                    for frame in &mut stacktrace.frames {
                        // Remove absolute paths to avoid leaking system information
                        if let Some(filename) = &mut frame.filename {
                            let project_root = env!("CARGO_MANIFEST_DIR");
                            if let Some(stripped) = filename.strip_prefix(project_root) {
                                *filename = stripped.trim_start_matches('/').to_string();
                            }
                        }
                    }
                }
            }
            Some(event)
        })),
        ..Default::default()
    };

    let guard = sentry::init(options);

    info!(
        environment = %environment,
        release = %release,
        traces_sample_rate = %traces_sample_rate,
        "Sentry initialized successfully"
    );

    // Set user context if available
    sentry::configure_scope(|scope| {
        if let Ok(user_id) = env::var("USER_ID") {
            scope.set_user(Some(sentry::User {
                id: Some(user_id),
                ..Default::default()
            }));
        }

        // Add default tags
        scope.set_tag("component", "oxy-core");
        if let Ok(node_env) = env::var("NODE_ENV") {
            scope.set_tag("node_env", &node_env);
        }
    });

    Some(guard)
}

/// Add context to Sentry scope for the current operation
pub fn add_operation_context(operation: &str, file_path: Option<&str>) {
    sentry::configure_scope(|scope| {
        scope.set_tag("operation", operation);
        if let Some(path) = file_path {
            scope.set_extra("file_path", path.into());
        }
        scope.set_level(Some(sentry::Level::Info));
    });
}

/// Add database context to Sentry scope
pub fn add_database_context(database_name: &str, query_type: Option<&str>) {
    sentry::configure_scope(|scope| {
        scope.set_tag("database", database_name);
        if let Some(qt) = query_type {
            scope.set_tag("query_type", qt);
        }
    });
}

/// Add workflow context to Sentry scope
pub fn add_workflow_context(workflow_name: &str, step: Option<&str>) {
    sentry::configure_scope(|scope| {
        scope.set_tag("workflow", workflow_name);
        if let Some(s) = step {
            scope.set_tag("workflow_step", s);
        }
    });
}

/// Add agent context to Sentry scope
pub fn add_agent_context(agent_name: &str, question: Option<&str>) {
    sentry::configure_scope(|scope| {
        scope.set_tag("agent", agent_name);
        if let Some(q) = question {
            // Truncate question to avoid large context
            let truncated_question = if q.len() > 200 {
                format!("{}...", &q[..200])
            } else {
                q.to_string()
            };
            scope.set_extra("agent_question", truncated_question.into());
        }
    });
}

/// Capture an error with additional context
pub fn capture_error_with_context(error: &dyn std::error::Error, context: &str) {
    sentry::configure_scope(|scope| {
        scope.set_extra("context", context.into());
    });
    sentry::capture_error(error);
}

/// Capture a message with additional context
pub fn capture_message_with_context(message: &str, level: sentry::Level, context: &str) {
    sentry::configure_scope(|scope| {
        scope.set_extra("context", context.into());
    });
    sentry::capture_message(message, level);
}

/// Create a breadcrumb for tracking user actions
pub fn add_breadcrumb(message: &str, category: &str, level: sentry::Level) {
    sentry::add_breadcrumb(sentry::Breadcrumb {
        ty: "user".into(),
        category: Some(category.into()),
        message: Some(message.into()),
        level,
        timestamp: std::time::SystemTime::now(),
        ..Default::default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_sentry_init_without_dsn() {
        // Test that Sentry doesn't initialize without DSN
        unsafe {
            env::remove_var("SENTRY_DSN");
        }
        let guard = init_sentry();
        assert!(guard.is_none());
    }

    #[test]
    fn test_sentry_init_with_empty_dsn() {
        // Test that Sentry doesn't initialize with empty DSN
        unsafe {
            env::set_var("SENTRY_DSN", "");
        }
        let guard = init_sentry();
        assert!(guard.is_none());
        unsafe {
            env::remove_var("SENTRY_DSN");
        }
    }

    #[test]
    fn test_sentry_context_helpers() {
        // Test that context helpers don't panic
        add_operation_context("test", Some("/path/to/file.sql"));
        add_database_context("test_db", Some("SELECT"));
        add_workflow_context("test_workflow", Some("step1"));
        add_agent_context("test_agent", Some("What is this?"));
        add_breadcrumb("Test action", "test", sentry::Level::Info);
    }

    #[test]
    fn test_capture_helpers() {
        use std::io;

        // Test error capture
        let error = io::Error::new(io::ErrorKind::NotFound, "Test error");
        capture_error_with_context(&error, "Test context");

        // Test message capture
        capture_message_with_context("Test message", sentry::Level::Warning, "Test context");
    }
}
