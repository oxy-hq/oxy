//! The device-code store's pure parts: the user code and the ask. What the
//! rows do — single collection, the verifier burning a request, the poll
//! interval — is driven over HTTP in `crates/server`'s `device_login` tests.

use serde_json::json;

use super::*;

#[test]
fn a_user_code_is_eight_consonants_shown_with_a_dash() {
    for _ in 0..200 {
        let code = new_user_code();
        assert_eq!(code.len(), USER_CODE_LEN, "{code}");
        assert!(
            code.bytes().all(|b| USER_CODE_ALPHABET.contains(&b)),
            "{code}"
        );
        assert_eq!(clean_user_code(&code), Some(code.clone()));
        let shown = display_user_code(&code);
        assert_eq!(shown.len(), USER_CODE_LEN + 1);
        assert_eq!(clean_user_code(&shown), Some(code));
    }
}

#[test]
fn a_user_code_is_read_as_a_person_types_it() {
    assert_eq!(clean_user_code("bcdf-ghjk"), Some("BCDFGHJK".into()));
    assert_eq!(clean_user_code(" BCDF GHJK "), Some("BCDFGHJK".into()));
    for bad in [
        "",
        "BCDF-GHJ",
        "BCDF-GHJKL",
        "BCDF-GHJA",
        "BCDF-GH0K",
        "BCDF_GHJK",
    ] {
        assert_eq!(clean_user_code(bad), None, "{bad}");
    }
}

#[test]
fn a_login_asks_for_nothing() {
    assert_eq!(clean_ask(None), Ok(json!({})));
    assert_eq!(clean_ask(Some(&json!(null))), Ok(json!({})));
    assert_eq!(clean_ask(Some(&json!({}))), Ok(json!({})));
    assert_eq!(asked_kind(&json!({})), AskedKind::Login);
    // A login does not carry a mint's fields: a request is one thing or the other.
    assert!(clean_ask(Some(&json!({ "hours": "8" }))).is_err());
}

#[test]
fn a_mint_asks_with_the_cli_auth_parameters() {
    let sandbox = json!({ "kind": "sandbox_agent", "apps": "acme/store,acme/ops", "hours": "8", "name": "nightly" });
    assert_eq!(clean_ask(Some(&sandbox)), Ok(sandbox.clone()));
    assert_eq!(asked_kind(&sandbox), AskedKind::Mint);
    let agent = json!({ "kind": "agent", "hours": "4", "standing": "1" });
    assert_eq!(clean_ask(Some(&agent)), Ok(agent.clone()));
    assert_eq!(asked_kind(&agent), AskedKind::Mint);
}

#[test]
fn an_ask_that_no_oxyc_sends_is_refused() {
    for bad in [
        json!("kind=agent"),
        json!({ "kind": "personal" }),
        json!({ "kind": "agent", "port": "8080" }),
        json!({ "kind": "agent", "hours": 8 }),
        json!({ "kind": "agent", "name": "a\nb" }),
        json!({ "kind": "agent", "hours": "1".repeat(9) }),
        json!({ "kind": "sandbox_agent", "apps": "a".repeat(2049) }),
    ] {
        assert!(clean_ask(Some(&bad)).is_err(), "{bad}");
    }
}
