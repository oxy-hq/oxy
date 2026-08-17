//! Dev-only sign-in bypass — `GET|POST /api/auth/dev-login`.
//!
//! Local development runs the production path (cloud mode, magic-link auth),
//! so anything driving a browser — Playwright MCP, a scratch Playwright
//! script, a headless CI probe — otherwise has to complete Google OAuth or
//! fish a token out of the `MAGIC_LINK_LOCAL_TEST` email preview. Neither is
//! scriptable. This endpoint mints exactly the session the real login flows
//! mint (`finalize_login` → JWT + the `oxy_session` cookie), for an identity
//! the operator pre-declared in the environment.
//!
//! Guard rails, in order of how much they matter:
//!
//! 1. **Off unless an allow-list resolves.** `OXY_DEV_LOGIN_EMAILS` supplies
//!    one in any build; in a **debug build only**, leaving it unset falls back
//!    to `OXY_GLOBAL_ADMINS`, so a dev box needs no configuration at all.
//!    Disabled, every verb 404s — a deployment that never sets the var does
//!    not advertise the route.
//!
//!    Two guards make that fallback safe, along different axes:
//!
//!    - **`debug_assertions`** covers shipped vs local. `OXY_GLOBAL_ADMINS`
//!      **is** set in production — it seeds the `app_admins` table — so an
//!      unconditional alias would turn every real deployment into an
//!      unauthenticated Global-Admin sign-in: one
//!      `POST {"email": "<any staff address>"}` and the caller holds Oxy's ops
//!      tier. Release binaries (prod, and every Docker image) see only the
//!      explicit var. A release binary run locally still works; it just names
//!      the var, exactly as before.
//!    - **Loopback** covers this machine vs the network. `serve` binds
//!      `0.0.0.0` by default and `.env.example` ships an uncommented roster, so
//!      a fallback honored for any peer would make a plain `cargo run serve` on
//!      café or office wifi vend staff sessions to every device on the network
//!      — with nothing typed and nothing in the flow announcing it. Only the
//!      explicit var, which somebody deliberately wrote, is served off-box.
//!
//!    The distinction throughout is *deliberateness*: an operator who typed a
//!    list of identities gets what they asked for; an inferred list never
//!    leaves the machine that inferred it.
//! 2. **Only pre-declared identities.** The caller may name an email, but it
//!    must already be in the allow-list; an unlisted address is a 403. So the
//!    worst a caller can do is become an identity the operator chose.
//! 3. **Loud.** Enabling it prints a warning at startup and logs a `warn!` on
//!    every issued session.
//! 4. **No drive-by sign-in.** Only `POST` sets the `SameSite=Lax` session
//!    cookie; `GET` returns the token in the body and nothing else, so a page
//!    a developer happens to visit cannot navigate them into a session.
//!
//! A dev box is cloud mode with non-prod secrets, so it is indistinguishable
//! from prod by `ServeMode` (see product-context.md) — the explicit env opt-in
//! is the gate, and a mode heuristic must not be added here.
//!
//! **Why not `#[cfg(debug_assertions)]` on the route** (considered, rejected):
//! it would make the bypass physically absent from shipped binaries, but
//! running a *released* binary locally is a supported dev path — `oxy start`
//! from an install, and every Docker image, is a release build. A compile-time
//! gate would make the documented workflow 404 with no explanation on exactly
//! those setups. The runtime gate is one deliberate env var either way.

use std::net::SocketAddr;
use std::sync::LazyLock;

use axum::{
    extract,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use entity::{prelude::Users, users, users::UserStatus};
use sea_orm::{ActiveValue, EntityTrait, Set};
use uuid::Uuid;

use oxy::database::{client::establish_connection, filters::UserQueryFilterExt};

use super::dto::{AuthResponse, DevLoginRequest};
use super::ops::{
    finalize_login, insert_user_or_fetch_existing, is_valid_email_format, login_response,
};

/// Comma-separated allow-list of sign-in identities. Setting it is what turns
/// the endpoint on; leaving it unset is what keeps it off everywhere else.
pub(crate) const DEV_LOGIN_EMAILS_ENV: &str = "OXY_DEV_LOGIN_EMAILS";

/// The staff roster that seeds `app_admins`. Read here **only** as a debug-build
/// convenience — see [`resolve_source`] for why that qualifier is load-bearing.
pub(crate) const GLOBAL_ADMINS_ENV: &str = "OXY_GLOBAL_ADMINS";

/// Where the allow-list came from. Not cosmetic: it decides who may *use* the
/// list, because only [`DevLoginSource::Explicit`] represents someone
/// deliberately turning the bypass on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DevLoginSource {
    /// `OXY_DEV_LOGIN_EMAILS` — an explicit opt-in, honored in every build.
    Explicit,
    /// `OXY_GLOBAL_ADMINS`, debug builds only.
    GlobalAdmins,
}

impl DevLoginSource {
    /// The variable to name in logs, so a reader never goes hunting for one
    /// they did not set.
    pub(crate) fn env_var(self) -> &'static str {
        match self {
            Self::Explicit => DEV_LOGIN_EMAILS_ENV,
            Self::GlobalAdmins => GLOBAL_ADMINS_ENV,
        }
    }

    /// Whether a session may only be issued to a loopback caller.
    ///
    /// The explicit var is a deliberate act — somebody typed a list of
    /// identities — so it is honored for any peer. That is the escape hatch
    /// for containers, remote dev boxes, and CI.
    ///
    /// The roster fallback is **not** a deliberate act: nobody typed
    /// anything, and `serve` binds `0.0.0.0` by default while `.env.example`
    /// ships an uncommented `OXY_GLOBAL_ADMINS`. Honoring those for a remote
    /// peer would make a plain `cargo run serve` on café or office wifi an
    /// unauthenticated global-admin vending machine for every device on the
    /// network — zero configuration performed, and nothing in the flow saying
    /// anything was enabled. Loopback keeps the zero-config win for the
    /// developer's own browser without ever leaving the machine.
    fn requires_loopback(self) -> bool {
        !matches!(self, Self::Explicit)
    }
}

/// The allow-list plus which variable produced it, so the startup banner can
/// name the right one instead of always blaming `OXY_DEV_LOGIN_EMAILS`.
struct DevLoginConfig {
    emails: Vec<String>,
    source: Option<DevLoginSource>,
}

/// Parsed once per process, which matches the "set it and restart" contract the
/// docs state. Re-reading per request would also re-emit the malformed-entry
/// warning on every `GET /auth/config` — which every client hits on boot — so a
/// single typo would become permanent log noise.
static DEV_LOGIN: LazyLock<DevLoginConfig> = LazyLock::new(|| {
    let (raw, source) = resolve_source(
        std::env::var(DEV_LOGIN_EMAILS_ENV).ok(),
        std::env::var(GLOBAL_ADMINS_ENV).ok(),
        cfg!(debug_assertions),
    );
    let emails = parse_dev_login_emails(raw.as_deref(), source);
    DevLoginConfig {
        source: source.filter(|_| !emails.is_empty()),
        emails,
    }
});

/// Which variable the allow-list came from, in order:
///
/// 1. `OXY_DEV_LOGIN_EMAILS` — the explicit opt-in, honored in every build.
///    Set-but-empty counts as "set", so `OXY_DEV_LOGIN_EMAILS=` is how you turn
///    the bypass **off** on a debug build without unsetting the staff roster.
/// 2. `OXY_GLOBAL_ADMINS` — debug builds only. A dev box already lists its staff
///    there, and a second identical list is one that goes stale. The pre-rename
///    `OXY_APP_ADMINS` is **not** a third rung: it is no longer read anywhere,
///    and `custom_apps_auth` says so at startup if it is still set.
///
/// `debug_build` is a parameter rather than a `cfg!` read inline so the release
/// behavior is testable from a debug test run — the case that matters most here
/// is precisely the one the test binary can't otherwise reach.
fn resolve_source(
    explicit: Option<String>,
    global_admins: Option<String>,
    debug_build: bool,
) -> (Option<String>, Option<DevLoginSource>) {
    if let Some(raw) = explicit {
        return (Some(raw), Some(DevLoginSource::Explicit));
    }
    if !debug_build {
        return (None, None);
    }
    // Never name a source we didn't actually read a value from, or the startup
    // banner would blame a variable nobody set.
    match global_admins {
        Some(raw) => (Some(raw), Some(DevLoginSource::GlobalAdmins)),
        None => (None, None),
    }
}

/// Loopback in the sense that matters: an IPv4-mapped `::ffff:127.0.0.1` is the
/// same machine as `127.0.0.1`, but `Ipv6Addr::is_loopback` alone says no.
fn is_loopback_peer(peer: SocketAddr) -> bool {
    peer.ip().to_canonical().is_loopback()
}

/// The peer address **when the listener supplied one**.
///
/// `ConnectInfo<SocketAddr>` is a mandatory extractor — axum 0.8 gives it no
/// optional impl — so a handler taking it directly returns a bare 500 on any
/// listener served without `into_make_service_with_connect_info`, and on any
/// router-level test that drives the service directly. The internal port was
/// exactly that listener. This extractor never rejects, so the handler decides
/// what an unknown peer means instead of the request dying with no explanation.
///
/// Unknown is treated as **not** loopback everywhere it's consulted: the only
/// thing loopback grants is the un-typed roster fallback, so failing closed
/// costs an explicit `OXY_DEV_LOGIN_EMAILS` and nothing else.
pub struct PeerAddr(pub Option<SocketAddr>);

impl PeerAddr {
    fn is_loopback(&self) -> bool {
        self.0.is_some_and(is_loopback_peer)
    }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for PeerAddr {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<extract::ConnectInfo<SocketAddr>>()
                .map(|extract::ConnectInfo(peer)| *peer),
        ))
    }
}

/// Whether the bypass is reachable *for this caller* — what `GET /auth/config`
/// must report, rather than the process-wide [`is_dev_login_enabled`].
///
/// `pub(crate)`, like everything else that reads the allow-list: nothing
/// outside this crate has a reason to ask, and an unreachable footgun beats a
/// signposted one.
///
/// The two have to agree or the 404 above buys nothing: `/auth/config` is
/// public, unauthenticated, and hit by every client on boot, so reporting a
/// flat `true` would tell the very off-box caller the 404 is hiding from that
/// there is a bypass here worth probing — and would render a Dev sign-in button
/// that can only 404 for them.
pub(crate) fn dev_login_reachable_by(peer: Option<SocketAddr>) -> bool {
    reachable(is_dev_login_enabled(), dev_login_is_loopback_only(), peer)
}

/// The decision itself, with the process-wide statics injected — same shape as
/// [`resolve_source`]'s `debug_build` parameter, and for the same reason: a
/// test that re-implements the expression cannot fail when the expression
/// changes.
fn reachable(enabled: bool, loopback_only: bool, peer: Option<SocketAddr>) -> bool {
    enabled && (!loopback_only || peer.is_some_and(is_loopback_peer))
}

/// The configured identities, normalized and validated. Empty ⇒ disabled.
pub(crate) fn dev_login_emails() -> &'static [String] {
    &DEV_LOGIN.emails
}

/// The env var that enabled the bypass, for the startup banner. `None` when off.
pub(crate) fn dev_login_source() -> Option<&'static str> {
    DEV_LOGIN.source.map(DevLoginSource::env_var)
}

/// Whether the active allow-list is only honored for loopback callers, so the
/// startup banner can say which of the two very different things is true.
pub(crate) fn dev_login_is_loopback_only() -> bool {
    DEV_LOGIN
        .source
        .is_some_and(DevLoginSource::requires_loopback)
}

/// Whether an allow-list resolved at all — **process-wide, ignoring who is
/// asking**. For the startup banner, and as one input to
/// [`dev_login_reachable_by`].
///
/// Not for a request path: an inferred roster is honored on loopback only, so
/// answering a caller from this alone re-opens the leak
/// [`dev_login_reachable_by`] exists to close. That function is what
/// `GET /auth/config` reports.
pub(crate) fn is_dev_login_enabled() -> bool {
    !dev_login_emails().is_empty()
}

fn parse_dev_login_emails(raw: Option<&str>, source: Option<DevLoginSource>) -> Vec<String> {
    // Name the variable the entries actually came from: a typo in
    // OXY_GLOBAL_ADMINS blaming OXY_DEV_LOGIN_EMAILS sends the reader looking
    // for a var they never set.
    let label = source.map_or(DEV_LOGIN_EMAILS_ENV, DevLoginSource::env_var);
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_lowercase)
        .filter(|entry| {
            let valid = is_valid_email_format(entry);
            if !valid {
                tracing::warn!("{label}: ignoring malformed entry {entry:?}");
            }
            valid
        })
        .collect()
}

/// Pick the identity to sign in as: the caller's choice when it is on the
/// allow-list, otherwise the first configured entry when the caller named
/// none. `None` means "asked for an address we will not issue".
fn resolve_dev_login_email(allowed: &[String], requested: Option<&str>) -> Option<String> {
    match requested.map(str::trim).filter(|email| !email.is_empty()) {
        None => allowed.first().cloned(),
        Some(requested) => {
            let requested = requested.to_lowercase();
            allowed.iter().find(|email| **email == requested).cloned()
        }
    }
}

/// The whole gate, decided before any database work: `404` when the bypass is
/// off *or* when an un-typed roster fallback is reached from off-box, `403`
/// when the caller named an address the operator did not declare.
///
/// The off-box case is a `404`, not a `403`, for the same reason "disabled" is:
/// from that peer's side the endpoint simply does not exist, and saying
/// "forbidden" would confirm there is a bypass here worth probing.
///
/// Caveat: `peer` is the socket's remote address, so a debug build behind a
/// reverse proxy sees the proxy and every caller looks local. Nothing in the
/// supported dev flow puts a proxy in front of `serve`, and release builds
/// never reach the fallback at all — but do not extend this check to trust a
/// forwarded-for header, which is caller-controlled.
fn resolve_or_refuse(
    allowed: &[String],
    requested: Option<&str>,
    source: Option<DevLoginSource>,
    peer_is_loopback: bool,
) -> Result<String, StatusCode> {
    if allowed.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    if source.is_some_and(DevLoginSource::requires_loopback) && !peer_is_loopback {
        tracing::warn!(
            "dev-login: refused a non-loopback caller — the allow-list came from \
             {} rather than an explicit {DEV_LOGIN_EMAILS_ENV}, so it is honored \
             on this machine only. Set {DEV_LOGIN_EMAILS_ENV} to serve other hosts.",
            source.map_or("(none)", DevLoginSource::env_var)
        );
        return Err(StatusCode::NOT_FOUND);
    }
    resolve_dev_login_email(allowed, requested).ok_or_else(|| {
        tracing::warn!(
            "dev-login: refused {:?} — not in {}",
            requested.unwrap_or_default(),
            source.map_or(DEV_LOGIN_EMAILS_ENV, DevLoginSource::env_var)
        );
        StatusCode::FORBIDDEN
    })
}

/// How the minted session is handed back.
///
/// `POST` gets the `oxy_session` cookie, like every real login flow. `GET`
/// deliberately does not: a top-level navigation IS a GET and the cookie is
/// `SameSite=Lax`, so any page a developer visits while a local server runs
/// could otherwise plant a session in their browser with a bare
/// `location = 'http://localhost:3000/api/auth/dev-login'`. GET exists for
/// scripts that want the token out of the body, and a script has no use for
/// the cookie — so withholding it costs nothing and closes login-CSRF.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SessionDelivery {
    CookieAndBody,
    BodyOnly,
}

/// `POST /auth/dev-login` — body `{"email": "..."}` (or `{}` for the first
/// configured identity). Used by the web-app's `/dev-login` page; sets the
/// session cookie.
pub async fn dev_login(
    peer: PeerAddr,
    headers: HeaderMap,
    extract::Json(req): extract::Json<DevLoginRequest>,
) -> Result<(HeaderMap, Json<AuthResponse>), StatusCode> {
    issue_dev_session(
        &headers,
        req.email.as_deref(),
        SessionDelivery::CookieAndBody,
        peer,
    )
    .await
}

/// `GET /auth/dev-login?email=...` — the token in the body, for tools that
/// would rather `curl` than run JavaScript. No `Set-Cookie`; see
/// [`SessionDelivery`].
pub async fn dev_login_get(
    peer: PeerAddr,
    headers: HeaderMap,
    extract::Query(req): extract::Query<DevLoginRequest>,
) -> Result<(HeaderMap, Json<AuthResponse>), StatusCode> {
    issue_dev_session(
        &headers,
        req.email.as_deref(),
        SessionDelivery::BodyOnly,
        peer,
    )
    .await
}

async fn issue_dev_session(
    headers: &HeaderMap,
    requested: Option<&str>,
    delivery: SessionDelivery,
    peer: PeerAddr,
) -> Result<(HeaderMap, Json<AuthResponse>), StatusCode> {
    let email = resolve_or_refuse(
        dev_login_emails(),
        requested,
        DEV_LOGIN.source,
        peer.is_loopback(),
    )?;

    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("dev-login: failed to establish database connection: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let existing = Users::find()
        .filter_by_email(&email)
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("dev-login: failed to query user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let user = match existing {
        // A deleted row is a deliberate state; the bypass must not resurrect
        // it, exactly as the OAuth handlers refuse to.
        Some(user) if user.status == UserStatus::Deleted => {
            tracing::warn!("dev-login: refused {email} — user is deleted");
            return Err(StatusCode::UNAUTHORIZED);
        }
        Some(user) => user,
        None => {
            let name = email.split('@').next().unwrap_or(&email).to_string();
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(email.clone()),
                name: Set(name),
                picture: Set(None),
                email_verified: Set(true),
                magic_link_token: ActiveValue::NotSet,
                magic_link_token_expires_at: ActiveValue::NotSet,
                status: Set(UserStatus::Active),
                created_at: ActiveValue::NotSet,
                last_login_at: ActiveValue::NotSet,
            };
            insert_user_or_fetch_existing(new_user, &email, &connection).await?
        }
    };

    tracing::warn!(
        "dev-login: issued a session for {email} without authentication ({} is set \
         — never set it on a shared deployment)",
        DEV_LOGIN
            .source
            .map_or(DEV_LOGIN_EMAILS_ENV, DevLoginSource::env_var)
    );

    let (token, user_info, orgs) = finalize_login(user, &connection).await?;
    Ok(match delivery {
        SessionDelivery::CookieAndBody => login_response(headers, token, user_info, orgs),
        SessionDelivery::BodyOnly => (
            HeaderMap::new(),
            Json(AuthResponse {
                token,
                user: user_info,
                orgs,
            }),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_env_disables_the_endpoint() {
        assert!(parse_dev_login_emails(None, None).is_empty());
        assert!(parse_dev_login_emails(Some(""), None).is_empty());
        assert!(parse_dev_login_emails(Some("  ,  "), None).is_empty());
    }

    #[test]
    fn entries_are_trimmed_lowercased_and_validated() {
        assert_eq!(
            parse_dev_login_emails(
                Some(" Dev@Oxy.local , not-an-email ,member@oxy.local"),
                None
            ),
            vec!["dev@oxy.local".to_string(), "member@oxy.local".to_string()]
        );
    }

    #[test]
    fn no_requested_email_uses_the_first_entry() {
        let allowed = parse_dev_login_emails(Some("dev@oxy.local,member@oxy.local"), None);
        assert_eq!(
            resolve_dev_login_email(&allowed, None),
            Some("dev@oxy.local".to_string())
        );
        assert_eq!(
            resolve_dev_login_email(&allowed, Some("   ")),
            Some("dev@oxy.local".to_string())
        );
    }

    #[test]
    fn requested_email_matches_case_insensitively() {
        let allowed = parse_dev_login_emails(Some("dev@oxy.local,member@oxy.local"), None);
        assert_eq!(
            resolve_dev_login_email(&allowed, Some("MEMBER@oxy.local")),
            Some("member@oxy.local".to_string())
        );
    }

    #[test]
    fn unlisted_email_is_refused_rather_than_falling_back() {
        let allowed = parse_dev_login_emails(Some("dev@oxy.local"), None);
        assert_eq!(
            resolve_dev_login_email(&allowed, Some("owner@oxy.tech")),
            None
        );
    }

    #[test]
    fn empty_allowlist_never_resolves() {
        assert_eq!(resolve_dev_login_email(&[], None), None);
        assert_eq!(resolve_dev_login_email(&[], Some("dev@oxy.local")), None);
    }

    // The gate itself, decided before any database work — so these cover the
    // two refusals the endpoint promises without needing Postgres. The third
    // refusal (401 for a deleted user) sits past `establish_connection` and is
    // not reachable from a unit test.

    #[test]
    fn disabled_is_a_404_not_a_403() {
        // 404 rather than 403 on purpose: a server without the env var must
        // not admit the route exists.
        assert_eq!(
            resolve_or_refuse(&[], None, None, true),
            Err(StatusCode::NOT_FOUND)
        );
        assert_eq!(
            resolve_or_refuse(&[], Some("dev@oxy.local"), None, true),
            Err(StatusCode::NOT_FOUND)
        );
    }

    // Where the allow-list comes from. The release-build cases are the reason
    // `resolve_source` takes `debug_build` instead of reading `cfg!` inline:
    // a debug test run could not otherwise exercise them, and they are exactly
    // the ones that would be a production auth bypass if they regressed.

    fn roster(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn explicit_var_wins_in_every_build() {
        for debug_build in [true, false] {
            assert_eq!(
                resolve_source(
                    roster("dev@oxy.local"),
                    roster("staff@oxy.tech"),
                    debug_build
                ),
                (roster("dev@oxy.local"), Some(DevLoginSource::Explicit))
            );
        }
    }

    #[test]
    fn debug_build_falls_back_to_the_staff_roster() {
        assert_eq!(
            resolve_source(None, roster("staff@oxy.tech"), true),
            (roster("staff@oxy.tech"), Some(DevLoginSource::GlobalAdmins))
        );
    }

    #[test]
    fn the_pre_rename_spelling_is_not_a_rung() {
        // OXY_APP_ADMINS was removed from every reader. Nothing here consults
        // it, so a .env that still uses only the old name resolves to nothing
        // — `custom_apps_auth::warn_on_removed_legacy_admins_env` is what tells
        // the operator, rather than a silent grant from a var we no longer read.
        assert_eq!(resolve_source(None, None, true), (None, None));
    }

    #[test]
    fn release_build_never_falls_back_to_the_roster() {
        // The whole point. OXY_GLOBAL_ADMINS is set on every real deployment,
        // so a release binary honoring it would mint Global-Admin sessions to
        // anyone who can reach the server — including, via kubectl
        // port-forward, callers who present as loopback.
        let resolved = resolve_source(None, roster("staff@oxy.tech"), false);
        assert_eq!(resolved, (None, None));
        assert!(parse_dev_login_emails(resolved.0.as_deref(), resolved.1).is_empty());
    }

    #[test]
    fn set_but_empty_disables_even_on_a_debug_build() {
        // OXY_DEV_LOGIN_EMAILS= is the off switch that doesn't require
        // unsetting the staff roster the rest of the app needs.
        let (raw, source) = resolve_source(Some(String::new()), roster("staff@oxy.tech"), true);
        assert!(parse_dev_login_emails(raw.as_deref(), source).is_empty());
    }

    #[test]
    fn nothing_set_is_disabled_in_both_builds() {
        assert_eq!(resolve_source(None, None, true), (None, None));
        assert_eq!(resolve_source(None, None, false), (None, None));
    }

    // Who may USE the list, as opposed to where it came from. `serve` binds
    // 0.0.0.0 by default, so an inferred roster that answered a LAN peer would
    // vend staff sessions to a coffee-shop network with nothing configured.

    #[test]
    fn an_inferred_roster_is_refused_off_box() {
        let allowed = parse_dev_login_emails(Some("staff@oxy.tech"), None);
        let source = DevLoginSource::GlobalAdmins;
        // 404, not 403 — from off-box the endpoint must not admit it exists.
        assert_eq!(
            resolve_or_refuse(&allowed, None, Some(source), false),
            Err(StatusCode::NOT_FOUND),
            "{source:?} must not be served to a remote peer"
        );
        assert_eq!(
            resolve_or_refuse(&allowed, None, Some(source), true),
            Ok("staff@oxy.tech".into()),
            "{source:?} must still work on the box itself"
        );
    }

    #[test]
    fn an_explicit_list_is_served_to_any_peer() {
        // The escape hatch: containers, remote dev boxes and CI need this, and
        // somebody deliberately typed the list.
        let allowed = parse_dev_login_emails(Some("dev@oxy.local"), None);
        assert_eq!(
            resolve_or_refuse(&allowed, None, Some(DevLoginSource::Explicit), false),
            Ok("dev@oxy.local".into())
        );
    }

    /// `/auth/config` must answer the same question the endpoint answers, or
    /// the 404 leaks through the neighbouring public route. Drives the real
    /// `reachable`, which `dev_login_reachable_by` is a thin wrapper over —
    /// only the address parsing is the test's own.
    fn reachable_by(enabled: bool, loopback_only: bool, peer: Option<&str>) -> bool {
        reachable(
            enabled,
            loopback_only,
            peer.map(|addr| addr.parse().unwrap()),
        )
    }

    #[test]
    fn config_hides_a_loopback_only_bypass_from_off_box_callers() {
        // The off-box caller gets a 404 from the endpoint; the config it hits
        // one request earlier must not contradict that.
        assert!(!reachable_by(true, true, Some("192.168.1.20:5000")));
        assert!(reachable_by(true, true, Some("127.0.0.1:5000")));
        // An explicit allow-list is a deliberate act, so it is advertised to
        // whoever can reach it — that's the container/CI escape hatch.
        assert!(reachable_by(true, false, Some("192.168.1.20:5000")));
        // Off entirely ⇒ never advertised, from anywhere.
        assert!(!reachable_by(false, false, Some("127.0.0.1:5000")));
    }

    #[test]
    fn unknown_peer_fails_closed() {
        // No connect-info (a listener without `into_make_service_with_connect_info`,
        // or a router-level test) must read as "not loopback", never as "trusted".
        assert!(!reachable_by(true, true, None));
        assert!(reachable_by(true, false, None));
        assert_eq!(
            resolve_or_refuse(
                &parse_dev_login_emails(Some("dev@oxy.local"), None),
                None,
                Some(DevLoginSource::GlobalAdmins),
                PeerAddr(None).is_loopback(),
            ),
            Err(StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn loopback_covers_ipv4_mapped_ipv6() {
        // ::ffff:127.0.0.1 is the same machine; Ipv6Addr::is_loopback alone
        // says otherwise, which would break the fallback on a dual-stack bind.
        for addr in ["127.0.0.1:1", "[::1]:1", "[::ffff:127.0.0.1]:1"] {
            assert!(
                is_loopback_peer(addr.parse().unwrap()),
                "{addr} should count as loopback"
            );
        }
        for addr in ["192.168.1.20:1", "10.0.0.5:1", "[2001:db8::1]:1"] {
            assert!(
                !is_loopback_peer(addr.parse().unwrap()),
                "{addr} must not count as loopback"
            );
        }
    }

    #[test]
    fn unlisted_email_is_a_403_never_a_silent_fallback() {
        let allowed = parse_dev_login_emails(Some("dev@oxy.local"), None);
        assert_eq!(
            resolve_or_refuse(
                &allowed,
                Some("owner@oxy.tech"),
                Some(DevLoginSource::Explicit),
                true
            ),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            resolve_or_refuse(&allowed, None, Some(DevLoginSource::Explicit), true),
            Ok("dev@oxy.local".into())
        );
    }
}
