//! Telling the host that a root run failed for good.
//!
//! A root run fails terminally in a handful of places — the coordinator after
//! its retry check, a root timeout, a recovery retirement or dead letter —
//! and the host (`oxy-app`) wants to tell somebody when it happens. This crate
//! cannot know who: it holds no users, orgs or inboxes. So the host installs
//! one [`RunFailureObserver`] at boot and each of those places calls
//! [`root_run_failed`] after the terminal status is written.
//!
//! The observer is handed the run id only and reads the row itself, so it
//! decides what counts (a cancel written as `failed`, a preview run, a child)
//! from the same record every writer left. Unset, every call is a no-op —
//! which is what tests and the CLI get.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

/// What the host does when a root run has failed with no retry coming.
#[async_trait]
pub trait RunFailureObserver: Send + Sync {
    /// Called after the run's terminal status is committed. Must not fail
    /// the caller: whatever goes wrong is the observer's to log.
    async fn root_run_failed(&self, run_id: &str);
}

static OBSERVER: OnceLock<Arc<dyn RunFailureObserver>> = OnceLock::new();

/// Install the observer. First call wins; the host does this once at boot.
pub fn set_run_failure_observer(observer: Arc<dyn RunFailureObserver>) {
    if OBSERVER.set(observer).is_err() {
        tracing::warn!("run failure observer already installed; ignoring the second");
    }
}

/// Tell the installed observer, if any, that root run `run_id` failed.
///
/// Awaited rather than spawned: the observer only records and queues (the
/// host's delivery is a durable task), and a spawn would be lost with the pod
/// between the terminal write and the record.
pub async fn root_run_failed(run_id: &str) {
    if let Some(observer) = OBSERVER.get() {
        observer.root_run_failed(run_id).await;
    }
}
