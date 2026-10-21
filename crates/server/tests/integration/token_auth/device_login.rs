//! `oxyc … --no-browser`: the device-code flow, request → approve → poll.
//!
//! The CLI starts a request and polls it, holding no credential; the person
//! finds it by its user code and decides it under their session, from any
//! device. What an approval mints is what the loopback exchange mints
//! (`sandbox_agent_cli`, `cli_login`): these tests cover what the device code
//! adds — the user code, the poll's states, the kind check, and the refusals
//! the CLI is told.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use entity::cli_device_requests::{Column, Entity as CliDeviceRequests};
use oxy_auth::token::cli_login::challenge_of;
use sea_orm::sea_query::Expr;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde_json::{Value, json};
use uuid::Uuid;

use super::sandbox_agent::{another_session, assert_refused, staff_with_app};
use super::stack::{Reach, flat_api, make_staff, mint};
use super::{audit_rows, call, fixture};

/// RFC 7636 appendix B's verifier.
pub(super) const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
pub(super) const HOST: &str = "cloud-box";

async fn start(ask: Option<Value>) -> (StatusCode, Value) {
    let mut body = json!({ "code_challenge": challenge_of(VERIFIER), "hostname": HOST });
    if let Some(ask) = ask {
        body["ask"] = ask;
    }
    call(flat_api(), "POST", "/auth/cli/device", &[], Some(body)).await
}

/// Start a request and return `(device_code, user_code)`.
pub(super) async fn started(ask: Option<Value>) -> (String, String) {
    let (status, body) = start(ask).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let text = |key: &str| body[key].as_str().expect(key).to_string();
    (text("device_code"), text("user_code"))
}

pub(super) async fn poll_with(device_code: &str, verifier: &str) -> (StatusCode, Value) {
    let body = json!({ "device_code": device_code, "code_verifier": verifier });
    call(flat_api(), "POST", "/auth/cli/device/poll", &[], Some(body)).await
}

/// Poll as the CLI does: after the interval. The clock is moved, not waited.
async fn poll(db: &DatabaseConnection, device_code: &str) -> (StatusCode, Value) {
    CliDeviceRequests::update_many()
        .col_expr(
            Column::LastPolledAt,
            Expr::value(None::<chrono::DateTime<chrono::FixedOffset>>),
        )
        .exec(db)
        .await
        .unwrap();
    poll_with(device_code, VERIFIER).await
}

pub(super) async fn page(
    cookie: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    call(flat_api(), method, uri, &[("cookie", cookie)], body).await
}

pub(super) async fn describe(cookie: &str, user_code: &str) -> (StatusCode, Value) {
    let uri = format!("/auth/cli/device/requests/{user_code}");
    page(cookie, "GET", &uri, None).await
}

pub(super) async fn approve(
    cookie: &str,
    user_code: &str,
    mint: Option<Value>,
) -> (StatusCode, Value) {
    let uri = format!("/auth/cli/device/requests/{user_code}/approve");
    let body = mint.map_or_else(|| json!({}), |mint| json!({ "mint": mint }));
    page(cookie, "POST", &uri, Some(body)).await
}

fn sandbox_ask() -> Value {
    json!({ "kind": "sandbox_agent", "apps": "acme/store", "hours": "2", "name": "nightly agent" })
}

fn sandbox_mint(apps: &[Uuid]) -> Value {
    json!({ "kind": "sandbox_agent", "apps": apps, "expires_in_hours": 2, "name": "nightly agent" })
}

#[tokio::test]
async fn a_request_approved_on_the_page_is_collected_once_by_polling() {
    let (fx, app) = staff_with_app().await;
    let (device_code, user_code) = started(Some(sandbox_ask())).await;
    assert!(
        user_code.len() == 9 && user_code.as_bytes()[4] == b'-',
        "{user_code}"
    );

    // The page reads the request back as the CLI asked it.
    let (status, request) = describe(&fx.cookie, &user_code.to_lowercase()).await;
    assert_eq!(status, StatusCode::OK, "{request}");
    assert_eq!(request["hostname"], HOST);
    assert_eq!(request["user_code"], json!(user_code));
    assert_eq!(request["ask"], sandbox_ask());

    let (status, waiting) = poll(&fx.db, &device_code).await;
    assert_eq!(status, StatusCode::OK, "{waiting}");
    assert_eq!(waiting, json!({ "status": "pending", "interval": 5 }));

    let (status, decided) = approve(&fx.cookie, &user_code, Some(sandbox_mint(&[app.id]))).await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    assert_eq!(decided["status"], "approved");
    // Decided: the page has nothing left to show.
    assert_eq!(
        describe(&fx.cookie, &user_code).await.0,
        StatusCode::NOT_FOUND
    );

    let (status, minted) = poll(&fx.db, &device_code).await;
    assert_eq!(status, StatusCode::OK, "{minted}");
    assert_eq!(minted["status"], "approved");
    let secret = minted["secret"].as_str().expect("the secret");
    assert!(secret.starts_with("oxy_sbx_"), "{secret}");
    assert_eq!(minted["token"]["name"], "nightly agent");
    assert_eq!(minted["token"]["grants"][0]["app_id"], json!(app.id));

    // Audited as the approver, saying how it was approved.
    let created = audit_rows(&fx.db, "token.created").await;
    let row = created
        .iter()
        .find(|r| r.metadata["token_kind"] == "sandbox_agent")
        .expect("the mint's row");
    assert_eq!(row.actor_user_id, Some(fx.user.id));
    assert_eq!(row.metadata["hostname"], HOST);
    assert_eq!(row.metadata["approval"], "device_code");
    assert_eq!(row.metadata["approver"], json!(fx.user.id));

    // Collected once.
    let again = poll(&fx.db, &device_code).await;
    assert_refused(
        again,
        StatusCode::BAD_REQUEST,
        "invalid_code",
        "a second collection",
    );
}

#[tokio::test]
async fn a_login_approved_by_device_code_is_the_hosts_login() {
    let fx = fixture().await;
    let (device_code, user_code) = started(None).await;
    let (status, decided) = approve(&fx.cookie, &user_code, None).await;
    assert_eq!(status, StatusCode::OK, "{decided}");

    let (status, minted) = poll(&fx.db, &device_code).await;
    assert_eq!(status, StatusCode::OK, "{minted}");
    assert!(minted["secret"].as_str().unwrap().starts_with("oxy_pat_"));
    assert_eq!(minted["token"]["name"], format!("oxyc on {HOST}"));
    assert_eq!(minted["token"]["all_access"], true);
}

/// RFC 8628 §5.4: whoever started a request collects it, and an approver
/// who was sent the link cannot tell who that is. So staff approving one
/// hand over at most what they reach as a member — never their staff reach.
#[tokio::test]
async fn a_token_collected_by_device_code_never_carries_staff_standing() {
    let fx = fixture().await;
    make_staff(&fx.db, fx.user.email.as_deref().unwrap()).await;

    let (device_code, user_code) = started(None).await;
    assert_eq!(
        approve(&fx.cookie, &user_code, None).await.0,
        StatusCode::OK
    );
    let (status, minted) = poll(&fx.db, &device_code).await;
    assert_eq!(status, StatusCode::OK, "{minted}");
    assert_eq!(minted["token"]["all_access"], true);
    assert_eq!(minted["token"]["platform"], false, "{minted}");
    assert_eq!(minted["token"]["partner"], false, "{minted}");

    // An agent token's approval may not ask for it, and nothing is decided.
    let (_, agent_code) = started(Some(json!({ "kind": "agent", "standing": "1" }))).await;
    let with_standing = json!({ "kind": "agent", "expires_in_hours": 2, "standing": true });
    let refused = approve(&fx.cookie, &agent_code, Some(with_standing)).await;
    assert_refused(
        refused,
        StatusCode::BAD_REQUEST,
        "invalid_agent_token",
        "standing",
    );
    assert_eq!(describe(&fx.cookie, &agent_code).await.0, StatusCode::OK);
}

#[tokio::test]
async fn an_approval_is_only_ever_of_the_kind_the_request_asked_for() {
    let (fx, app) = staff_with_app().await;
    let (_, sandbox_code) = started(Some(sandbox_ask())).await;
    // An agent's narrow request is never approved as an all-access login.
    let refused = approve(&fx.cookie, &sandbox_code, None).await;
    assert_eq!(refused.0, StatusCode::BAD_REQUEST, "{}", refused.1);
    let agent = json!({ "kind": "agent", "expires_in_hours": 2 });
    let refused = approve(&fx.cookie, &sandbox_code, Some(agent.clone())).await;
    assert_eq!(refused.0, StatusCode::BAD_REQUEST, "{}", refused.1);
    // A login is never approved as a mint.
    let (_, login_code) = started(None).await;
    let refused = approve(&fx.cookie, &login_code, Some(sandbox_mint(&[app.id]))).await;
    assert_eq!(refused.0, StatusCode::BAD_REQUEST, "{}", refused.1);

    // A page that sent the wrong body refused nothing: both still wait.
    assert_eq!(describe(&fx.cookie, &sandbox_code).await.0, StatusCode::OK);
    assert_eq!(describe(&fx.cookie, &login_code).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_refusal_on_the_page_stops_the_cli_with_its_reason() {
    let (fx, app) = staff_with_app().await;

    // Declined by the person.
    let (device_code, user_code) = started(Some(sandbox_ask())).await;
    let uri = format!("/auth/cli/device/requests/{user_code}/deny");
    let (status, decided) = page(&fx.cookie, "POST", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{decided}");
    let denied = poll(&fx.db, &device_code).await;
    assert!(
        denied.1["error"].as_str().unwrap().contains("declined"),
        "{}",
        denied.1
    );
    assert_refused(denied, StatusCode::BAD_REQUEST, "access_denied", "declined");

    // Refused under the session: the page is told, and so is the CLI.
    let (_, stranger) = another_session(&fx.db, "stranger").await;
    let (device_code, user_code) = started(Some(sandbox_ask())).await;
    let refused = approve(&stranger, &user_code, Some(sandbox_mint(&[app.id]))).await;
    assert_refused(
        refused,
        StatusCode::NOT_FOUND,
        "app_not_found",
        "a stranger",
    );
    let denied = poll(&fx.db, &device_code).await;
    let reason = denied.1["error"].as_str().unwrap_or_default().to_string();
    assert!(reason.contains("sandbox agent tokens"), "{reason}");
    assert_refused(
        denied,
        StatusCode::BAD_REQUEST,
        "access_denied",
        "refused mint",
    );
}

#[tokio::test]
async fn a_wrong_verifier_burns_the_request() {
    let fx = fixture().await;
    let (device_code, user_code) = started(None).await;
    let wrong = "x".repeat(43);
    let refused = poll_with(&device_code, &wrong).await;
    assert_refused(
        refused,
        StatusCode::BAD_REQUEST,
        "invalid_code",
        "a wrong verifier",
    );

    // Burned: even approved, the right verifier collects nothing.
    assert_eq!(
        approve(&fx.cookie, &user_code, None).await.0,
        StatusCode::OK
    );
    let refused = poll(&fx.db, &device_code).await;
    assert_refused(
        refused,
        StatusCode::BAD_REQUEST,
        "invalid_code",
        "after a wrong verifier",
    );
    let unknown = poll(&fx.db, "no-such-device-code").await;
    assert_refused(
        unknown,
        StatusCode::BAD_REQUEST,
        "invalid_code",
        "an unknown code",
    );
}

#[tokio::test]
async fn polling_faster_than_the_interval_is_told_to_slow_down() {
    let _fx = fixture().await;
    let (device_code, _) = started(None).await;
    let (status, first) = poll_with(&device_code, VERIFIER).await;
    assert_eq!(
        (status, &first["status"]),
        (StatusCode::OK, &json!("pending"))
    );
    let (status, second) = poll_with(&device_code, VERIFIER).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["status"], "slow_down");
}

#[tokio::test]
async fn an_undecided_request_expires() {
    let fx = fixture().await;
    let (device_code, user_code) = started(None).await;
    let lapsed = (Utc::now() - Duration::seconds(1)).fixed_offset();
    CliDeviceRequests::update_many()
        .col_expr(Column::ExpiresAt, Expr::value(lapsed))
        .exec(&fx.db)
        .await
        .unwrap();

    assert_eq!(
        describe(&fx.cookie, &user_code).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        approve(&fx.cookie, &user_code, None).await.0,
        StatusCode::NOT_FOUND
    );
    let expired = poll(&fx.db, &device_code).await;
    assert_refused(expired, StatusCode::BAD_REQUEST, "expired_token", "lapsed");
}

#[tokio::test]
async fn only_a_browser_session_reads_or_decides_a_request() {
    let fx = fixture().await;
    let (_, user_code) = started(None).await;
    let (_, pat) = mint(&fx.db, fx.user.id, Reach::all_access()).await;
    let bearer = format!("Bearer {pat}");
    let uri = format!("/auth/cli/device/requests/{user_code}");
    let read = call(flat_api(), "GET", &uri, &[("authorization", &bearer)], None).await;
    assert_refused(
        read,
        StatusCode::FORBIDDEN,
        "session_required",
        "a PAT reads",
    );
    let approve_uri = format!("{uri}/approve");
    let decided = call(
        flat_api(),
        "POST",
        &approve_uri,
        &[("authorization", &bearer)],
        Some(json!({})),
    )
    .await;
    assert_refused(
        decided,
        StatusCode::FORBIDDEN,
        "session_required",
        "a PAT approves",
    );
}

#[tokio::test]
async fn a_request_no_oxyc_sends_is_refused_at_the_start() {
    let _fx = fixture().await;
    let bad_ask = start(Some(json!({ "kind": "personal" }))).await;
    assert_eq!(bad_ask.0, StatusCode::BAD_REQUEST, "{}", bad_ask.1);
    let unknown_field = start(Some(json!({ "kind": "agent", "port": "8080" }))).await;
    assert_eq!(
        unknown_field.0,
        StatusCode::BAD_REQUEST,
        "{}",
        unknown_field.1
    );
    let body = json!({ "code_challenge": "short", "hostname": HOST });
    let bad_challenge = call(flat_api(), "POST", "/auth/cli/device", &[], Some(body)).await;
    assert_eq!(
        bad_challenge.0,
        StatusCode::BAD_REQUEST,
        "{}",
        bad_challenge.1
    );
}
