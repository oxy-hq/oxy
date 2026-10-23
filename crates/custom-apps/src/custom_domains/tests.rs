use async_trait::async_trait;

use super::dns::{LookupError, TxtResolver};
use super::*;

/// Answers one fixed result for every name, and records the name asked.
struct Fixed {
    answer: Result<Vec<String>, String>,
    asked: std::sync::Mutex<Vec<String>>,
}

impl Fixed {
    fn ok(values: &[&str]) -> Self {
        Self {
            answer: Ok(values.iter().map(|s| s.to_string()).collect()),
            asked: Default::default(),
        }
    }
    fn failing(reason: &str) -> Self {
        Self {
            answer: Err(reason.to_string()),
            asked: Default::default(),
        }
    }
}

#[async_trait]
impl TxtResolver for Fixed {
    async fn txt(&self, name: &str) -> Result<Vec<String>, LookupError> {
        self.asked.lock().unwrap().push(name.to_string());
        self.answer.clone().map_err(LookupError)
    }
}

fn row(domain: &str, token: &str) -> app_custom_domains::Model {
    app_custom_domains::Model {
        id: Uuid::new_v4(),
        app_id: Uuid::new_v4(),
        domain: domain.to_string(),
        verification_token: token.to_string(),
        verified_at: None,
        last_checked_at: None,
        last_check_error: None,
        created_by: None,
        created_at: Utc::now().fixed_offset(),
    }
}

#[tokio::test]
async fn a_matching_txt_record_proves_the_domain() {
    let resolver = Fixed::ok(&["unrelated=1", "oxygen-verification=tok123"]);
    let outcome = check_challenge(&resolver, &row("app.acme.com", "tok123")).await;
    assert_eq!(outcome, Ok(()));
    assert_eq!(
        *resolver.asked.lock().unwrap(),
        vec!["_oxygen-challenge.app.acme.com".to_string()]
    );
}

#[tokio::test]
async fn another_rows_token_does_not_prove_this_one() {
    // The token is per row: a record published for an earlier claim, or for
    // another org's claim on the same name, proves nothing here.
    let resolver = Fixed::ok(&["oxygen-verification=someone-else"]);
    let err = check_challenge(&resolver, &row("app.acme.com", "tok123"))
        .await
        .unwrap_err();
    assert!(err.contains("does not match"), "{err}");
    assert!(err.contains("oxygen-verification=tok123"), "{err}");
}

#[tokio::test]
async fn a_missing_record_says_so() {
    let resolver = Fixed::ok(&[]);
    let err = check_challenge(&resolver, &row("app.acme.com", "tok123"))
        .await
        .unwrap_err();
    assert!(err.starts_with("No TXT record found"), "{err}");
}

#[tokio::test]
async fn a_failed_lookup_is_reported_not_read_as_missing() {
    let resolver = Fixed::failing("resolver returned DNS status 2");
    let err = check_challenge(&resolver, &row("app.acme.com", "tok123"))
        .await
        .unwrap_err();
    assert!(err.contains("DNS lookup failed"), "{err}");
}

#[test]
fn tokens_are_random_and_dns_safe() {
    let (a, b) = (new_token(), new_token());
    assert_ne!(a, b);
    assert_eq!(a.len(), 32);
    assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn a_domain_token_is_bound_to_its_domain() {
    use entity::api_tokens;
    use oxy_auth::token::credential::source;
    let now = Utc::now();
    let token = api_tokens::Model {
        id: Uuid::new_v4(),
        kind: "personal".to_string(),
        principal_user_id: Uuid::new_v4(),
        name: session::token_name("app.acme.com"),
        display_prefix: String::new(),
        last_four: String::new(),
        token_hash: vec![0; 32],
        all_access: false,
        platform: false,
        partner: false,
        expires_at: Some((now + chrono::Duration::hours(1)).fixed_offset()),
        last_used_at: None,
        created_at: now.fixed_offset(),
        created_by: None,
        revoked_at: None,
        revoked_by: None,
        revoke_reason: None,
        source: source::CUSTOM_DOMAIN.to_string(),
        legacy_api_key_id: None,
        trust_policy_id: None,
        oidc_claims: None,
    };
    assert!(session::is_domain_token_for(&token, "app.acme.com", now));
    assert!(!session::is_domain_token_for(&token, "other.acme.com", now));

    let login_link = api_tokens::Model {
        source: source::UI.to_string(),
        ..token.clone()
    };
    assert!(!session::is_domain_token_for(
        &login_link,
        "app.acme.com",
        now
    ));

    let expired = api_tokens::Model {
        expires_at: Some((now - chrono::Duration::seconds(1)).fixed_offset()),
        ..token.clone()
    };
    assert!(!session::is_domain_token_for(&expired, "app.acme.com", now));

    let revoked = api_tokens::Model {
        revoked_at: Some(now.fixed_offset()),
        ..token
    };
    assert!(!session::is_domain_token_for(&revoked, "app.acme.com", now));
}
