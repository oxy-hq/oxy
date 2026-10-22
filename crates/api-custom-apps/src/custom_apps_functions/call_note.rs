//! What the host learned about one `ctx.*` call while running it — the
//! namespace, verb and table a database op resolved — handed back to the
//! broker for the call's trace record (`oxy_custom_apps::call_trace`).
//!
//! One writer, two sinks: `data_audit::record_db_span` and
//! `record_db_namespace` put the same identifier-gated values on the host-op
//! span and into this note, so the waterfall cannot say more than the span.
//! The note is a task-local the broker scopes around each host-call task
//! (`runtime::host_call_task`); every call site runs inline in that task's
//! future. Outside a scope — a unit test calling the host directly — writing
//! the note does nothing.

use std::cell::RefCell;

use super::host_call_attrs::identifier_like;

/// The identifiers a database op recorded on its span.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CallNote {
    pub namespace: Option<String>,
    pub verb: Option<String>,
    pub table: Option<String>,
    /// The host held the call by a mechanism of its own and answered as if
    /// it ran (`host::env_guard`): a staging fetch's 409, a held email.
    pub held: bool,
}

impl CallNote {
    /// `namespace.table`, either alone, or `None`.
    pub(crate) fn target(&self) -> Option<String> {
        match (&self.namespace, &self.table) {
            (Some(ns), Some(table)) => Some(format!("{ns}.{table}")),
            (Some(one), None) | (None, Some(one)) => Some(one.clone()),
            (None, None) => None,
        }
    }
}

tokio::task_local! {
    static CALL_NOTE: RefCell<CallNote>;
}

/// Run `future` with an empty note in scope, and hand back the note with its
/// output.
pub(crate) async fn scoped<F: std::future::Future>(future: F) -> (F::Output, CallNote) {
    CALL_NOTE
        .scope(RefCell::new(CallNote::default()), async move {
            let output = future.await;
            let note = CALL_NOTE.with(|note| note.take());
            (output, note)
        })
        .await
}

fn kept(value: Option<&str>) -> Option<String> {
    value.filter(|v| identifier_like(v)).map(str::to_string)
}

/// Note that the host held this call rather than performing it.
pub(crate) fn note_held() {
    let _ = CALL_NOTE.try_with(|note| note.borrow_mut().held = true);
}

/// Note what a statement touched. Gated as the span is.
pub(crate) fn note_db(namespace: Option<&str>, verb: &str, table: Option<&str>) {
    let _ = CALL_NOTE.try_with(|note| {
        let mut note = note.borrow_mut();
        if let Some(ns) = kept(namespace) {
            note.namespace = Some(ns);
        }
        if let Some(verb) = kept(Some(verb)) {
            note.verb = Some(verb);
        }
        if let Some(table) = kept(table) {
            note.table = Some(table);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_scoped_note_holds_only_identifiers_and_no_scope_is_a_no_op() {
        note_db(Some("outside"), "SELECT", Some("t"));
        let ((), note) = scoped(async {
            note_db(Some("app_store"), "UPDATE", Some("orders"));
            note_db(Some("x'y"), "a b", Some("p@q"));
            note_held();
        })
        .await;
        assert!(note.held);
        assert_eq!(note.target().as_deref(), Some("app_store.orders"));
        assert_eq!(note.verb.as_deref(), Some("UPDATE"));
    }
}
