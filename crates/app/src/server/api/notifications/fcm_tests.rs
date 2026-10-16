use jsonwebtoken::{DecodingKey, Validation};

use super::*;
use crate::server::api::notifications::native_push::tests::{rsa_key, signal};

fn service_account(pem: &str) -> String {
    json!({
        "type": "service_account",
        "project_id": "oxygen-test-1",
        "private_key_id": "kid-1",
        "private_key": pem,
        "client_email": "push@oxygen-test-1.iam.gserviceaccount.com",
        "token_uri": "https://evil.example/token",
    })
    .to_string()
}

#[test]
fn the_assertion_is_rs256_for_the_fixed_token_endpoint() {
    let (pem, public) = rsa_key();
    let fcm = Fcm::configure(Some(&service_account(&pem)))
        .ok()
        .expect("configures");
    let jwt = fcm.assertion(1_800_000_000).expect("signs");

    let header = jsonwebtoken::decode_header(&jwt).expect("a header");
    assert_eq!(header.alg, Algorithm::RS256);
    assert_eq!(header.kid.as_deref(), Some("kid-1"));

    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = false;
    validation.set_audience(&[TOKEN_URI]);
    let data =
        jsonwebtoken::decode::<Value>(&jwt, &DecodingKey::from_rsa_der(&public), &validation)
            .expect("verifies with the public key");
    assert_eq!(
        data.claims,
        json!({
            "iss": "push@oxygen-test-1.iam.gserviceaccount.com",
            "scope": "https://www.googleapis.com/auth/firebase.messaging",
            // Never the key file's `token_uri`.
            "aud": "https://oauth2.googleapis.com/token",
            "iat": 1_800_000_000u64,
            "exp": 1_800_000_000u64 + 3600,
        })
    );
}

#[test]
fn the_message_names_the_kind_and_row_and_carries_no_content() {
    let s = signal("delivery.received");
    let id = s.notification_id.to_string();
    assert_eq!(
        message("fcm-token", &s),
        json!({
            "message": {
                "token": "fcm-token",
                "notification": { "title": "Oxygen", "body": "Delivery received" },
                "data": { "kind": "delivery.received", "id": id },
                "android": {
                    "priority": "high",
                    "ttl": "14400s",
                    "collapse_key": id,
                    "notification": { "channel_id": "default" },
                },
            }
        })
    );
}

fn error(code: u16, status: &str, details: Value, message: &str) -> String {
    json!({ "error": { "code": code, "status": status, "message": message, "details": details } })
        .to_string()
}

#[test]
fn only_errors_about_the_token_delete_it() {
    assert_eq!(
        classify(200, r#"{"name":"projects/p/messages/1"}"#),
        Outcome::Delivered
    );

    let unregistered = error(
        404,
        "NOT_FOUND",
        json!([{ "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError", "errorCode": "UNREGISTERED" }]),
        "Requested entity was not found.",
    );
    assert_eq!(classify(404, &unregistered), Outcome::Gone);

    let bad_token_field = error(
        400,
        "INVALID_ARGUMENT",
        json!([
            { "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError", "errorCode": "INVALID_ARGUMENT" },
            { "@type": "type.googleapis.com/google.rpc.BadRequest",
              "fieldViolations": [{ "field": "message.token", "description": "Invalid registration token" }] },
        ]),
        "Invalid value at 'message.token'",
    );
    assert_eq!(classify(400, &bad_token_field), Outcome::Gone);

    let bad_token_message = error(
        400,
        "INVALID_ARGUMENT",
        json!([]),
        "The registration token is not a valid FCM registration token",
    );
    assert_eq!(classify(400, &bad_token_message), Outcome::Gone);

    // Not about the token: these must never erase a device.
    let bad_payload = error(
        400,
        "INVALID_ARGUMENT",
        json!([{ "@type": "type.googleapis.com/google.rpc.BadRequest",
                 "fieldViolations": [{ "field": "message.android.ttl", "description": "bad" }] }]),
        "Invalid value at 'message.android.ttl'",
    );
    for (status, body) in [
        (400, bad_payload),
        // A bare 404 (a wrong project, a wrong path) names no token.
        (404, String::new()),
        (
            404,
            error(
                404,
                "NOT_FOUND",
                json!([]),
                "Requested entity was not found.",
            ),
        ),
        (401, error(401, "UNAUTHENTICATED", json!([]), "auth")),
        (
            403,
            error(
                403,
                "PERMISSION_DENIED",
                json!([{ "errorCode": "SENDER_ID_MISMATCH" }]),
                "mismatch",
            ),
        ),
        (
            429,
            error(
                429,
                "RESOURCE_EXHAUSTED",
                json!([{ "errorCode": "QUOTA_EXCEEDED" }]),
                "q",
            ),
        ),
        (503, error(503, "UNAVAILABLE", json!([]), "down")),
    ] {
        assert!(
            matches!(classify(status, &body), Outcome::Failed(_)),
            "{status} {body} must fail, not delete"
        );
    }
}

#[test]
fn configuration_refuses_what_it_cannot_use() {
    assert_eq!(Fcm::configure(None).err(), Some(ConfigError::Missing));
    assert!(matches!(
        Fcm::configure(Some("{}")).err(),
        Some(ConfigError::Json(_))
    ));

    let (pem, _) = rsa_key();
    let mut sa: Value = serde_json::from_str(&service_account(&pem)).expect("json");
    sa["type"] = json!("authorized_user");
    assert!(matches!(
        Fcm::configure(Some(&sa.to_string())).err(),
        Some(ConfigError::Json(_))
    ));

    let mut sa: Value = serde_json::from_str(&service_account(&pem)).expect("json");
    sa["project_id"] = json!("../other/x");
    assert_eq!(
        Fcm::configure(Some(&sa.to_string())).err(),
        Some(ConfigError::ProjectId)
    );

    assert!(matches!(
        Fcm::configure(Some(&service_account("-- not a key --"))).err(),
        Some(ConfigError::PrivateKey(_))
    ));
}
