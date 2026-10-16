use jsonwebtoken::{DecodingKey, Validation};

use super::*;
use crate::server::api::notifications::native_push::tests::{ec_key, signal};

fn env(pem: &str) -> ApnsEnv {
    ApnsEnv {
        key_p8: Some(pem.to_string()),
        key_id: Some("ABC123DEFG".into()),
        team_id: Some("TEAM123456".into()),
        topic: None,
        sandbox: false,
    }
}

fn configured() -> (Apns, Vec<u8>) {
    let (pem, public) = ec_key();
    let apns = Apns::configure(env(&pem))
        .ok()
        .expect("a fresh P-256 key configures");
    (apns, public)
}

#[test]
fn the_provider_token_is_es256_with_kid_and_iss_iat_only() {
    let (apns, public) = configured();
    let jwt = apns.bearer(1_800_000_000).expect("signs");

    let header = jsonwebtoken::decode_header(&jwt).expect("a header");
    assert_eq!(header.alg, Algorithm::ES256);
    assert_eq!(header.kid.as_deref(), Some("ABC123DEFG"));
    assert_eq!(header.typ, None, "Apple documents {{alg, kid}} only");

    // Verified against the key's public half: the signature is real ES256.
    let mut validation = Validation::new(Algorithm::ES256);
    validation.required_spec_claims.clear();
    validation.validate_exp = false;
    let data = jsonwebtoken::decode::<Value>(&jwt, &DecodingKey::from_ec_der(&public), &validation)
        .expect("verifies with the public key");
    assert_eq!(
        data.claims,
        json!({ "iss": "TEAM123456", "iat": 1_800_000_000u64 }),
        "exactly iss and iat"
    );
}

#[test]
fn the_provider_token_is_cached_for_50_minutes_then_re_signed() {
    let (apns, _) = configured();
    let t0 = 1_800_000_000;
    let first = apns.bearer(t0).expect("signs");
    assert_eq!(
        apns.bearer(t0 + JWT_REFRESH_SECS - 1).expect("cached"),
        first
    );
    let refreshed = apns.bearer(t0 + JWT_REFRESH_SECS).expect("re-signs");
    assert_ne!(refreshed, first, "a 50-minute-old token must be re-signed");
    // Under Apple's one-hour cap, over its 20-minute throttle.
    const { assert!(JWT_REFRESH_SECS < 60 * 60 && JWT_REFRESH_SECS > 20 * 60) };

    apns.forget_bearer();
    assert_ne!(
        apns.bearer(t0 + JWT_REFRESH_SECS).expect("signs"),
        refreshed
    );
}

#[test]
fn the_headers_are_the_alert_set_with_the_row_as_collapse_id() {
    let id = Uuid::new_v4();
    let headers = headers("tech.oxy.oxygen", "JWT", id, 1_000);
    assert_eq!(
        headers,
        vec![
            ("apns-topic", "tech.oxy.oxygen".to_string()),
            ("apns-push-type", "alert".to_string()),
            ("apns-priority", "10".to_string()),
            ("apns-expiration", (1_000 + TTL_SECONDS).to_string()),
            ("apns-collapse-id", id.to_string()),
            ("authorization", "bearer JWT".to_string()),
        ]
    );
}

#[test]
fn the_payload_names_the_kind_and_row_and_carries_no_content() {
    let s = signal("list_overdue");
    assert_eq!(
        payload(&s),
        json!({
            "aps": {
                "alert": { "title": "Oxygen", "body": "List overdue" },
                "sound": "default",
                "mutable-content": 1,
                "thread-id": "list_overdue",
            },
            "oxy": { "kind": "list_overdue", "id": s.notification_id.to_string() },
        })
    );
}

#[test]
fn only_reasons_about_the_token_delete_it() {
    assert_eq!(classify(200, None), Outcome::Delivered);
    assert_eq!(classify(410, Some("Unregistered")), Outcome::Gone);
    assert_eq!(classify(410, None), Outcome::Gone);
    assert_eq!(classify(400, Some("Unregistered")), Outcome::Gone);
    // Configuration and transient failures must never erase a device. That
    // includes `BadDeviceToken`: Apple also answers it for a token from the
    // other environment, so a sandbox/production mismatch must not deregister.
    for (status, reason) in [
        (400, Some("BadDeviceToken")),
        (400, Some("DeviceTokenNotForTopic")),
        (400, Some("BadTopic")),
        (400, None),
        (403, Some("InvalidProviderToken")),
        (403, Some("ExpiredProviderToken")),
        (429, Some("TooManyRequests")),
        (500, Some("InternalServerError")),
        (503, Some("ServiceUnavailable")),
    ] {
        assert!(
            matches!(classify(status, reason), Outcome::Failed(_)),
            "{status} {reason:?} must fail, not delete"
        );
    }
    assert_eq!(
        reason_of(r#"{"reason":"BadDeviceToken"}"#).as_deref(),
        Some("BadDeviceToken")
    );
    assert_eq!(reason_of(""), None);
}

#[tokio::test]
async fn a_non_hex_token_is_refused_before_any_request() {
    let (apns, _) = configured();
    let s = signal("k");
    // Would otherwise be a path segment on Apple's host.
    let out = apns.send_one("../../x?y", &s, "JWT", 0).await;
    assert_eq!(out, Outcome::Failed("not a hex APNs device token".into()));
}

#[test]
fn configuration_is_all_or_nothing_and_checks_the_ids() {
    let (pem, _) = ec_key();
    let mut partial = env(&pem);
    partial.team_id = None;
    assert_eq!(Apns::configure(partial).err(), Some(ConfigError::Missing));
    assert_eq!(
        Apns::configure(ApnsEnv::default()).err(),
        Some(ConfigError::Missing)
    );

    let mut bad_kid = env(&pem);
    bad_kid.key_id = Some("short".into());
    assert_eq!(Apns::configure(bad_kid).err(), Some(ConfigError::KeyId));

    let mut bad_team = env(&pem);
    bad_team.team_id = Some("TEAM-12345".into());
    assert_eq!(Apns::configure(bad_team).err(), Some(ConfigError::TeamId));

    assert!(matches!(
        Apns::configure(env("-- not a key --")).err(),
        Some(ConfigError::PrivateKey(_))
    ));
}

#[test]
fn the_topic_defaults_and_sandbox_picks_the_host() {
    let (pem, _) = ec_key();
    let apns = Apns::configure(env(&pem)).ok().expect("configures");
    assert_eq!(apns.topic, "tech.oxy.oxygen");
    assert_eq!(apns.host, "https://api.push.apple.com");

    let mut sandbox = env(&pem);
    sandbox.sandbox = true;
    sandbox.topic = Some("tech.oxy.oxygen.dev".into());
    // An escaped PEM, as a secret store hands it over, is accepted too.
    sandbox.key_p8 = Some(pem.replace('\n', "\\n"));
    let apns = Apns::configure(sandbox).ok().expect("configures");
    assert_eq!(apns.topic, "tech.oxy.oxygen.dev");
    assert_eq!(apns.host, "https://api.sandbox.push.apple.com");
}
