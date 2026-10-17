//! This process's `LISTEN oxy_app_tail` connection: every envelope any
//! process `NOTIFY`s reaches this process's tail subscribers through it
//! ([`bus::deliver_local`]), this process's own envelopes included.
//!
//! Modelled on chat's listener (`chat::delivery`), with two differences:
//!
//! - **Opened on the first tail, not at boot.** Most processes never hold a
//!   tail, and a `LISTEN` connection is one more slot on the database. Boot
//!   only [`install`]s the settings — the connection factory and TLS posture
//!   `router::entry` derives for chat and the task router (this surface may
//!   not import that module: `tests/custom_apps/custom_apps_boundary.rs`) —
//!   and [`ensure_started`] opens the connection once.
//! - **It says when it was away.** `NOTIFY` is not queued for a session that
//!   is not listening, so whatever was sent while the connection was down is
//!   gone. Each attach bumps [`Link::epoch`]; a stream that sees the epoch
//!   move tells its reader with a `dropped` marker.

use std::sync::OnceLock;
use std::time::Duration;

use agentic_runtime::router::{ListenerConfigFactory, TlsVerification};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use super::{bus, event};

/// Whether this process is listening, and how many times it has attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Link {
    pub attached: bool,
    /// Bumped on every successful `LISTEN`. A subscriber that saw one epoch
    /// and now sees a later one may have missed what was sent in between.
    pub epoch: u64,
}

struct Settings {
    factory: ListenerConfigFactory,
    verification: TlsVerification,
    shutdown: CancellationToken,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();
static LINK: OnceLock<watch::Sender<Link>> = OnceLock::new();

/// Why no tail can be opened in this process.
#[derive(Debug, thiserror::Error)]
pub enum Unavailable {
    #[error("this process was started without the tail listener's settings")]
    NotInstalled,
    #[error("the tail listener did not attach within {0:?}")]
    NotAttached(Duration),
}

/// How long a new stream waits for the listener's first attach.
const ATTACH_TIMEOUT: Duration = Duration::from_secs(10);

/// Retry delay after a dropped or failed connection; doubles to the cap.
const RETRY_FIRST: Duration = Duration::from_millis(500);
const RETRY_CAP: Duration = Duration::from_secs(5);

/// Give this process the listener's connection settings and its shutdown
/// signal. Called from `router::entry` beside chat's listener; the first
/// call wins, as `new_agentic_state` runs once per router.
pub fn install(
    factory: ListenerConfigFactory,
    verification: TlsVerification,
    shutdown: CancellationToken,
) {
    let _ = SETTINGS.set(Settings {
        factory,
        verification,
        shutdown,
    });
}

/// The shutdown signal [`install`] was given, for the streams to end on.
pub fn shutdown_signal() -> Option<CancellationToken> {
    SETTINGS.get().map(|s| s.shutdown.clone())
}

/// Start the listener if it is not running, and wait until it is attached.
/// The receiver reports every later detach and re-attach.
pub async fn ensure_started() -> Result<watch::Receiver<Link>, Unavailable> {
    let settings = SETTINGS.get().ok_or(Unavailable::NotInstalled)?;
    let mut started = false;
    let link = LINK.get_or_init(|| {
        started = true;
        watch::channel(Link::default()).0
    });
    if started {
        // A process-wide connection with no request behind it: no hub to
        // carry, and nothing a tenant sent rides on its errors.
        tokio::spawn(run(
            settings.factory.clone(),
            settings.verification,
            settings.shutdown.clone(),
            link.clone(),
        ));
    }
    let mut rx = link.subscribe();
    let attached = tokio::time::timeout(ATTACH_TIMEOUT, rx.wait_for(|l| l.attached))
        .await
        .is_ok_and(|waited| waited.is_ok());
    if !attached {
        return Err(Unavailable::NotAttached(ATTACH_TIMEOUT));
    }
    Ok(rx)
}

/// Connect, listen and fan out until shutdown, reconnecting on every drop.
async fn run(
    factory: ListenerConfigFactory,
    verification: TlsVerification,
    shutdown: CancellationToken,
    link: watch::Sender<Link>,
) {
    let mut delay = RETRY_FIRST;
    loop {
        if shutdown.is_cancelled() {
            return;
        }
        match listen(&factory, verification, &shutdown, &link).await {
            Ok(()) => return,
            Err(e) => {
                link.send_modify(|l| l.attached = false);
                warn!(error = %e, "tail listener dropped; reconnecting in {delay:?}");
            }
        }
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(RETRY_CAP);
    }
}

/// One connection's life: `LISTEN`, mark attached, deliver until it drops.
async fn listen(
    factory: &ListenerConfigFactory,
    verification: TlsVerification,
    shutdown: &CancellationToken,
    link: &watch::Sender<Link>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (client, mut connection) =
        agentic_runtime::router::connect_listener(factory, verification).await?;
    // Notifications surface through the connection future's `poll_message`,
    // so the connection is driven here rather than spawned and forgotten.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        use futures::stream::StreamExt;
        let mut stream = futures::stream::poll_fn(move |cx| connection.poll_message(cx));
        while let Some(message) = stream.next().await {
            match message {
                Ok(tokio_postgres::AsyncMessage::Notification(n)) => {
                    if tx.send(n.payload().to_string()).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(error = %e, "tail listener connection error");
                    return;
                }
            }
        }
    });
    client
        .batch_execute(&format!("LISTEN {}", event::NOTIFY_CHANNEL))
        .await?;
    link.send_modify(|l| {
        l.attached = true;
        l.epoch += 1;
    });
    debug!("tail listener attached");
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            payload = rx.recv() => match payload {
                Some(payload) => deliver(&payload),
                None => return Err("listener connection closed".into()),
            },
        }
    }
}

/// Hand one payload to this process's subscribers. One this build cannot
/// read is a newer producer's, not a reason to drop the connection.
fn deliver(payload: &str) {
    match event::unpack(payload) {
        Some(envelope) => bus::deliver_local(envelope),
        None => debug!(
            bytes = payload.len(),
            "tail: an envelope this build cannot read"
        ),
    }
}
