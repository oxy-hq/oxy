//! Opening a Postgres connection so that the error we report is the
//! informative one.
//!
//! **`tokio_postgres::Config::connect` keeps only the LAST error.** It resolves
//! the host to every address and tries each in turn, overwriting the stored
//! error on every failure. A host that publishes both A and AAAA records, on a
//! pod with no IPv6 route, therefore ends on the IPv6 attempt:
//! `Network is unreachable`, rendered by `Error::connect`'s `Display` as the
//! bare `error connecting to server`. Whatever the IPv4 attempt returned — a
//! refusal from the server itself — is gone.
//!
//! That is exactly how a Neon project over its quota read as a networking fault
//! for two days. Neon answered every connection with
//!
//! ```text
//! ERROR: Your account or project has exceeded the quota
//! ```
//!
//! and the one line that said so was overwritten by the IPv6 attempt that
//! followed it.
//!
//! [`connect`] resolves and tries each address itself, so it can choose which
//! failure to report:
//!
//! 1. The FIRST attempt that got further than reaching the address — a server
//!    `ErrorResponse`, a TLS failure, a rejected password. Those say what is
//!    actually wrong, so they outrank any io error, earlier or later.
//! 2. Otherwise every attempt was unreachable, and the FIRST one is reported
//!    with its io cause and the number of addresses tried.

use std::net::IpAddr;

use tokio_postgres::config::Host;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::{Client, Config, Connection, Socket};

use crate::pg_error::pg_error_message;

/// `Display` of an `Error::connect`: the TCP connection never opened. Compared
/// as text because `tokio_postgres::Error` exposes no kind accessor. If a
/// release changes the wording the failure is treated as informative, so the
/// worst outcome is reporting the first error without the address count.
const UNREACHABLE: &str = "error connecting to server";

/// Why one address failed.
enum Failure {
    Driver(tokio_postgres::Error),
    /// The name did not resolve, or resolved to nothing.
    Resolve(String),
}

impl Failure {
    /// The TCP connection never opened, so nothing about the server is known.
    fn is_unreachable(&self) -> bool {
        match self {
            Self::Resolve(_) => true,
            Self::Driver(e) => e.as_db_error().is_none() && e.to_string() == UNREACHABLE,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Driver(e) => pg_error_message(e),
            Self::Resolve(why) => format!("{UNREACHABLE}: {why}"),
        }
    }
}

struct Attempt {
    /// `host:port` or `ip:port`, for the "tried N addresses" tail.
    target: String,
    failure: Failure,
}

/// Collapse a non-empty list of failed attempts into the message to report.
fn explain(attempts: &[Attempt]) -> String {
    if let Some(a) = attempts.iter().find(|a| !a.failure.is_unreachable()) {
        return a.failure.message();
    }
    let Some(first) = attempts.first() else {
        return "no address to connect to".to_string();
    };
    let base = first.failure.message();
    if attempts.len() == 1 {
        return format!("{base} ({})", first.target);
    }
    let tried = attempts
        .iter()
        .map(|a| a.target.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{base} (first of {} addresses tried: {tried})",
        attempts.len()
    )
}

/// A [`Config`] that connects to exactly one address.
///
/// `host` stays the original name even when `ip` is set: tokio-postgres takes
/// the TLS server name from `host` and dials `hostaddr`, so certificate
/// verification and SNI keep working against an address we resolved ourselves.
///
/// Every setting is copied from `base` by hand because `Config` can append
/// hosts but never remove them. A setting added to `tokio_postgres::Config`
/// later is not carried over until it is listed here.
fn single_address(base: &Config, host: Option<&Host>, ip: Option<IpAddr>, port: u16) -> Config {
    let mut c = Config::new();
    match host {
        Some(Host::Tcp(name)) => {
            c.host(name);
        }
        #[cfg(unix)]
        Some(Host::Unix(path)) => {
            c.host_path(path);
        }
        None => {}
    }
    if let Some(ip) = ip {
        c.hostaddr(ip);
    }
    c.port(port);
    if let Some(v) = base.get_user() {
        c.user(v);
    }
    if let Some(v) = base.get_password() {
        c.password(v);
    }
    if let Some(v) = base.get_dbname() {
        c.dbname(v);
    }
    if let Some(v) = base.get_options() {
        c.options(v);
    }
    if let Some(v) = base.get_application_name() {
        c.application_name(v);
    }
    c.ssl_mode(base.get_ssl_mode());
    c.ssl_negotiation(base.get_ssl_negotiation());
    if let Some(v) = base.get_connect_timeout() {
        c.connect_timeout(*v);
    }
    if let Some(v) = base.get_tcp_user_timeout() {
        c.tcp_user_timeout(*v);
    }
    c.keepalives(base.get_keepalives());
    c.keepalives_idle(base.get_keepalives_idle());
    if let Some(v) = base.get_keepalives_interval() {
        c.keepalives_interval(v);
    }
    if let Some(v) = base.get_keepalives_retries() {
        c.keepalives_retries(v);
    }
    c.target_session_attrs(base.get_target_session_attrs());
    c.channel_binding(base.get_channel_binding());
    c
}

/// One address to dial: the configured host (kept for TLS), the resolved IP,
/// and the port. `ip` is `None` for a Unix socket path.
type Target<'a> = (Option<&'a Host>, Option<IpAddr>, u16);

/// Whether `config` has a host/hostaddr/port shape tokio-postgres itself
/// rejects, in which case it should be left to report that.
fn is_malformed(config: &Config) -> bool {
    let (hosts, hostaddrs, ports) = (
        config.get_hosts(),
        config.get_hostaddrs(),
        config.get_ports(),
    );
    let count = hosts.len().max(hostaddrs.len());
    count == 0
        || (!hosts.is_empty() && !hostaddrs.is_empty() && hosts.len() != hostaddrs.len())
        || (ports.len() > 1 && ports.len() != count)
}

/// Expand the configured hosts into the addresses to dial, in the order
/// tokio-postgres would try them. A name that does not resolve becomes a
/// failed attempt rather than an early return, so it is ranked with the rest.
async fn resolve_targets(config: &Config) -> (Vec<Target<'_>>, Vec<Attempt>) {
    let (hosts, hostaddrs, ports) = (
        config.get_hosts(),
        config.get_hostaddrs(),
        config.get_ports(),
    );
    let mut targets: Vec<Target<'_>> = Vec::new();
    let mut attempts: Vec<Attempt> = Vec::new();
    for i in 0..hosts.len().max(hostaddrs.len()) {
        let host = hosts.get(i);
        let port = ports
            .get(i)
            .or_else(|| ports.first())
            .copied()
            .unwrap_or(5432);
        if let Some(ip) = hostaddrs.get(i) {
            targets.push((host, Some(*ip), port));
            continue;
        }
        let Some(Host::Tcp(name)) = host else {
            // A socket path has nothing to resolve.
            targets.push((host, None, port));
            continue;
        };
        let target = format!("{name}:{port}");
        match tokio::net::lookup_host((name.as_str(), port)).await {
            Ok(resolved) => {
                let before = targets.len();
                targets.extend(resolved.map(|a| (host, Some(a.ip()), port)));
                if targets.len() == before {
                    attempts.push(Attempt {
                        target,
                        failure: Failure::Resolve(format!("{name} resolved to no addresses")),
                    });
                }
            }
            Err(e) => attempts.push(Attempt {
                target,
                failure: Failure::Resolve(format!("could not resolve {name}: {e}")),
            }),
        }
    }
    (targets, attempts)
}

/// Connect like [`Config::connect`], but report the most informative failure
/// rather than the last one. The `Err` is a finished human-readable message.
pub(crate) async fn connect<T>(
    config: &Config,
    tls: T,
) -> Result<(Client, Connection<Socket, T::Stream>), String>
where
    T: MakeTlsConnect<Socket> + Clone,
{
    if is_malformed(config) {
        // Let tokio-postgres produce its own configuration error.
        return config.connect(tls).await.map_err(|e| pg_error_message(&e));
    }
    let (targets, mut attempts) = resolve_targets(config).await;
    for (host, ip, port) in targets {
        let target = match (ip, host) {
            (Some(ip), _) => format!("{ip}:{port}"),
            (None, Some(Host::Tcp(name))) => format!("{name}:{port}"),
            _ => format!("socket:{port}"),
        };
        match single_address(config, host, ip, port)
            .connect(tls.clone())
            .await
        {
            Ok(connected) => return Ok(connected),
            Err(e) => attempts.push(Attempt {
                target,
                failure: Failure::Driver(e),
            }),
        }
    }
    Err(explain(&attempts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_postgres::NoTls;
    use tokio_postgres::config::SslMode;

    /// How the fake server answers a startup packet.
    #[derive(Clone, Copy)]
    enum Reply {
        /// `ErrorResponse` with this message, as Neon sends over its quota.
        Refuse(&'static str),
        /// `AuthenticationOk`, `BackendKeyData`, `ReadyForQuery`.
        Accept,
    }

    const QUOTA: &str = "Your account or project has exceeded the quota";

    /// A one-message-type Postgres server: reads the startup packet, answers,
    /// and holds the socket open. Returns the bound port.
    async fn fake_server(reply: Reply) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    // Startup packet: Int32 length (self-inclusive) + body.
                    let mut len = [0u8; 4];
                    if sock.read_exact(&mut len).await.is_err() {
                        return;
                    }
                    let mut body = vec![0u8; u32::from_be_bytes(len) as usize - 4];
                    if sock.read_exact(&mut body).await.is_err() {
                        return;
                    }
                    let mut out = Vec::new();
                    match reply {
                        Reply::Refuse(message) => {
                            let mut fields = Vec::new();
                            for (tag, text) in [(b'S', "FATAL"), (b'C', "XX000"), (b'M', message)] {
                                fields.push(tag);
                                fields.extend_from_slice(text.as_bytes());
                                fields.push(0);
                            }
                            fields.push(0);
                            out.push(b'E');
                            out.extend_from_slice(&(fields.len() as u32 + 4).to_be_bytes());
                            out.extend_from_slice(&fields);
                        }
                        Reply::Accept => {
                            out.extend_from_slice(&[b'R', 0, 0, 0, 8, 0, 0, 0, 0]);
                            out.extend_from_slice(&[b'K', 0, 0, 0, 12, 0, 0, 0, 1, 0, 0, 0, 2]);
                            out.extend_from_slice(&[b'Z', 0, 0, 0, 5, b'I']);
                        }
                    }
                    let _ = sock.write_all(&out).await;
                    // Hold the connection open until the client drops it.
                    let _ = sock.read(&mut [0u8; 1]).await;
                });
            }
        });
        port
    }

    /// A port nothing listens on: connecting is refused at the TCP level,
    /// which is an io error and says nothing about the server.
    fn dead_port() -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    }

    /// One logical host, one address per entry. `hostaddr` skips DNS, so the
    /// list is exactly what the test names; the shared `host` is what TLS
    /// would validate against.
    fn config_for(ports: &[u16]) -> Config {
        let mut c = Config::new();
        c.user("u").dbname("d").ssl_mode(SslMode::Disable);
        for port in ports {
            c.host("db.example.test");
            c.hostaddr("127.0.0.1".parse().unwrap());
            c.port(*port);
        }
        c
    }

    async fn failure_of(config: &Config) -> String {
        match connect(config, NoTls).await {
            Ok(_) => panic!("expected the connection to fail"),
            Err(message) => message,
        }
    }

    /// The incident: the server refuses the first address, and the next one is
    /// unreachable. tokio-postgres reports the unreachable one, which reads as
    /// a network fault; the refusal is what must reach the caller.
    #[tokio::test]
    async fn a_servers_refusal_outranks_a_later_unreachable_address() {
        let refusing = fake_server(Reply::Refuse(QUOTA)).await;
        let message = failure_of(&config_for(&[refusing, dead_port()])).await;
        assert!(message.contains(QUOTA), "refusal was lost: {message}");
        assert!(message.contains("XX000"), "no SQLSTATE: {message}");
        assert!(
            !message.contains("error connecting to server"),
            "reported the unreachable address instead: {message}"
        );
    }

    /// Order must not matter: an unreachable address first does not hide the
    /// refusal behind it.
    #[tokio::test]
    async fn a_servers_refusal_outranks_an_earlier_unreachable_address() {
        let refusing = fake_server(Reply::Refuse(QUOTA)).await;
        let message = failure_of(&config_for(&[dead_port(), refusing])).await;
        assert!(message.contains(QUOTA), "refusal was lost: {message}");
    }

    /// With nothing but io errors, report the first, with the io cause that
    /// `Error::connect`'s `Display` drops, and how many addresses were tried.
    #[tokio::test]
    async fn all_unreachable_reports_the_first_error_with_its_cause_and_the_count() {
        let (first, second) = (dead_port(), dead_port());
        let message = failure_of(&config_for(&[first, second])).await;
        assert!(
            message.starts_with("error connecting to server: "),
            "the io cause is missing: {message}"
        );
        assert!(message.contains("(os error"), "no os error text: {message}");
        assert!(
            message.contains("first of 2 addresses tried"),
            "no address count: {message}"
        );
        assert!(
            message.contains(&format!("127.0.0.1:{first}"))
                && message.contains(&format!("127.0.0.1:{second}")),
            "addresses not named: {message}"
        );
    }

    /// A good address after a bad one still connects: choosing among errors
    /// must not change which attempts are made. Uses host names rather than
    /// `hostaddr`, so the `lookup_host` path is exercised as well.
    #[tokio::test]
    async fn a_working_second_address_still_connects() {
        let live = fake_server(Reply::Accept).await;
        let mut c = Config::new();
        c.user("u").dbname("d").ssl_mode(SslMode::Disable);
        for port in [dead_port(), live] {
            c.host("127.0.0.1");
            c.port(port);
        }
        let (client, _connection) = connect(&c, NoTls)
            .await
            .unwrap_or_else(|e| panic!("second address should connect: {e}"));
        assert!(!client.is_closed());
    }

    #[test]
    fn an_empty_attempt_list_does_not_panic() {
        assert_eq!(explain(&[]), "no address to connect to");
    }
}
