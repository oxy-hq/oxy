//! The registered [`Push`]: one transport per `device_tokens.platform`.
//!
//! Web Push, APNs and FCM are configured independently, and each is handed
//! only its own platform's rows — an APNs token posted to a Web Push endpoint
//! would be a POST to a URL that is not one. A row whose platform has no
//! configured transport is left alone and counted in the log line, so "this
//! user's phone never buzzes" is answerable from the logs.

use std::sync::Arc;

use entity::device_tokens;
use tracing::info;

use super::apns::{Apns, PLATFORM_APNS};
use super::deliver::{Push, PushReport, PushSignal, set_push};
use super::fcm::{Fcm, PLATFORM_FCM};
use super::web_push::{PLATFORM_WEB, WebPush};

struct Route {
    platform: &'static str,
    transport: Arc<dyn Push>,
}

#[derive(Default)]
pub struct RoutingPush {
    routes: Vec<Route>,
}

impl RoutingPush {
    /// Route `platform`'s rows to `transport`.
    pub fn with(mut self, platform: &'static str, transport: Arc<dyn Push>) -> Self {
        self.routes.push(Route {
            platform,
            transport,
        });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// The transports, for the boot log line.
    pub fn names(&self) -> Vec<&'static str> {
        self.routes.iter().map(|r| r.transport.name()).collect()
    }

    /// Every transport the environment configures. Each one that is not says
    /// why in its own startup warning.
    pub fn from_env() -> Self {
        let mut routing = Self::default();
        if let Some(w) = WebPush::from_env() {
            routing = routing.with(PLATFORM_WEB, Arc::new(w));
        }
        if let Some(a) = Apns::from_env() {
            routing = routing.with(PLATFORM_APNS, Arc::new(a));
        }
        if let Some(f) = Fcm::from_env() {
            routing = routing.with(PLATFORM_FCM, Arc::new(f));
        }
        routing
    }
}

#[async_trait::async_trait]
impl Push for RoutingPush {
    async fn send(&self, tokens: &[device_tokens::Model], signal: &PushSignal) -> PushReport {
        let sends = self.routes.iter().filter_map(|route| {
            let rows: Vec<device_tokens::Model> = tokens
                .iter()
                .filter(|t| t.platform == route.platform)
                .cloned()
                .collect();
            (!rows.is_empty()).then(|| async move { route.transport.send(&rows, signal).await })
        });
        let reports = futures::future::join_all(sends).await;

        let unrouted = tokens
            .iter()
            .filter(|t| !self.routes.iter().any(|r| r.platform == t.platform))
            .count();
        if unrouted > 0 {
            info!(
                user = %signal.user_id,
                devices = unrouted,
                "push skipped devices on a platform with no configured transport"
            );
        }
        reports
            .into_iter()
            .fold(PushReport::default(), |sum, r| PushReport {
                delivered: sum.delivered + r.delivered,
                failed: sum.failed + r.failed,
            })
    }

    fn name(&self) -> &'static str {
        "routing"
    }

    fn configured(&self) -> bool {
        self.routes.iter().any(|r| r.transport.configured())
    }

    fn handles(&self, platform: &str) -> bool {
        self.routes
            .iter()
            .any(|r| r.platform == platform && r.transport.configured())
    }
}

/// Register every configured transport, if there is at least one. Returns
/// whether anything registered, for the caller's boot log line.
pub fn register_if_configured() -> bool {
    let routing = RoutingPush::from_env();
    if routing.is_empty() {
        return false;
    }
    let names = routing.names().join(", ");
    set_push(Arc::new(routing));
    info!(transports = %names, "push registered");
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::server::api::notifications::native_push::tests::{row, signal};

    /// Records the tokens it was handed and reports each as delivered, or as
    /// failed when `fail` is set.
    struct Fake {
        name: &'static str,
        fail: bool,
        seen: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(name: &'static str, fail: bool) -> Arc<Self> {
            Arc::new(Self {
                name,
                fail,
                seen: Mutex::new(Vec::new()),
            })
        }
        fn seen(&self) -> Vec<String> {
            self.seen.lock().expect("lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl Push for Fake {
        async fn send(&self, tokens: &[device_tokens::Model], _: &PushSignal) -> PushReport {
            self.seen
                .lock()
                .expect("lock")
                .extend(tokens.iter().map(|t| t.token.clone()));
            match self.fail {
                true => PushReport {
                    delivered: 0,
                    failed: tokens.len(),
                },
                false => PushReport {
                    delivered: tokens.len(),
                    failed: 0,
                },
            }
        }
        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn each_transport_gets_only_its_platform_and_reports_are_summed() {
        let (web, apns, fcm) = (
            Fake::new("web", false),
            Fake::new("apns", false),
            Fake::new("fcm", true),
        );
        let routing = RoutingPush::default()
            .with("web", web.clone())
            .with("apns", apns.clone())
            .with("fcm", fcm.clone());
        let tokens = [
            row("web", "w1"),
            row("apns", "a1"),
            row("fcm", "f1"),
            row("apns", "a2"),
            row("web", "w2"),
        ];
        let report = routing.send(&tokens, &signal("k")).await;
        assert_eq!(web.seen(), ["w1", "w2"]);
        assert_eq!(apns.seen(), ["a1", "a2"]);
        assert_eq!(fcm.seen(), ["f1"]);
        assert_eq!(
            report,
            PushReport {
                delivered: 4,
                failed: 1
            }
        );
    }

    #[tokio::test]
    async fn an_unconfigured_platform_is_skipped_and_an_idle_transport_not_called() {
        let (web, apns) = (Fake::new("web", false), Fake::new("apns", false));
        let routing = RoutingPush::default()
            .with("web", web.clone())
            .with("apns", apns.clone());
        // No FCM transport: the fcm row goes nowhere; no apns rows: not called.
        let tokens = [row("web", "w1"), row("fcm", "f1")];
        let report = routing.send(&tokens, &signal("k")).await;
        assert_eq!(web.seen(), ["w1"]);
        assert!(apns.seen().is_empty());
        assert_eq!(
            report,
            PushReport {
                delivered: 1,
                failed: 0
            }
        );
        assert_eq!(routing.names(), ["web", "apns"]);
        assert!(routing.configured());
    }

    #[test]
    fn nothing_configured_is_empty_and_not_configured() {
        let routing = RoutingPush::default();
        assert!(routing.is_empty());
        assert!(!routing.configured());
    }

    #[test]
    fn a_platform_handles_only_where_its_own_sender_is_configured() {
        // Web Push set up, APNs/FCM not: a phone's rows reach no sender, so the
        // ledger says `not_configured` rather than `no_devices`.
        let routing = RoutingPush::default().with(PLATFORM_WEB, Fake::new("web", false));
        assert!(routing.handles(PLATFORM_WEB));
        assert!(!routing.handles(PLATFORM_APNS));
        assert!(!routing.handles(PLATFORM_FCM));
    }
}
