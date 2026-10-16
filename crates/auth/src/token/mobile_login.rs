//! Signing in the Oxygen mobile app: a PKCE authorization code bound to the
//! app's redirect URI (RFC 8252, native apps).
//!
//! 1. The app opens `/mobile-auth?code_challenge&redirect_uri&state` in the
//!    system browser. The browser, under its session, asks for a **code**
//!    bound to the challenge **and** the redirect URI ([`authorize`]).
//!    Single use, five minutes.
//! 2. The browser navigates to `<redirect_uri>?code=…&state=…`; the OS hands
//!    that URL to the app. The code alone is worthless.
//! 3. The app redeems `code + verifier + redirect_uri` ([`redeem`]) and the
//!    handler mints a login session for the code's user.
//!
//! Only `oxygen://` redirect URIs are accepted ([`clean_redirect_uri`]): the
//! scheme the iOS and Android apps register. Anything else would turn the
//! approval page into a way to post a code to an arbitrary site.
//!
//! ## The code lives in `cli_auth_codes`
//!
//! It is the same five-minute, single-use, PKCE-bound handoff as an `oxyc
//! login` code, so it shares that table, its sweep and its spend-first
//! redemption ([`cli_login::spend`]). It is stored under its own hash
//! ([`CODE_DOMAIN`]) for the reason a mint code and a browser ticket are: the
//! `oxyc login` exchange, in this release or the last, never computes that
//! hash, so a mobile code can not be redeemed there for an all-access token —
//! and an `oxyc` code can not be redeemed here for a session. The row's `mint`
//! names its kind and the redirect URI; `hostname` is empty.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use entity::cli_auth_codes;
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::cli_login::{self, CODE_TTL_SECS, VERIFIER_LEN, challenge_of};

/// The one scheme a mobile redirect URI may use.
pub const REDIRECT_SCHEME: &str = "oxygen";
/// Longer than any deep link the apps register; bounds what is stored.
const MAX_REDIRECT_URI_LEN: usize = 512;
/// What a mobile code is prefixed with before it is hashed. See the module docs.
const CODE_DOMAIN: &str = "oxy-mobile-login:";
/// `cli_auth_codes.mint.kind` of a mobile code's row.
const CODE_KIND: &str = "mobile_login";

/// The redirect URI as it is stored and later compared, or `None` when it is
/// not one the Oxygen app could have registered: `oxygen://` followed by
/// printable ASCII, with no query, fragment or user info. The approval page
/// appends `?code=…&state=…`, so a URI that already carries a query would come
/// back malformed.
pub fn clean_redirect_uri(raw: &str) -> Option<String> {
    let uri = raw.trim();
    let (scheme, rest) = uri.split_once("://")?;
    let ok = scheme.eq_ignore_ascii_case(REDIRECT_SCHEME)
        && uri.len() <= MAX_REDIRECT_URI_LEN
        && uri.bytes().all(|b| b.is_ascii_graphic())
        && !rest.contains(['?', '#', '@', '\\']);
    ok.then(|| uri.to_string())
}

fn hash_code(code: &str) -> Vec<u8> {
    Sha256::digest(format!("{CODE_DOMAIN}{code}").as_bytes()).to_vec()
}

/// Issue a code for `user_id`, redeemable once against `code_challenge` and
/// `redirect_uri` until it expires. The caller has cleaned both
/// ([`cli_login::clean_challenge`], [`clean_redirect_uri`]).
pub async fn authorize<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    code_challenge: &str,
    redirect_uri: &str,
) -> Result<String, OxyError> {
    let now = Utc::now();
    cli_login::sweep(db, now).await;
    let code = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    cli_auth_codes::ActiveModel {
        code_hash: Set(hash_code(&code)),
        user_id: Set(user_id),
        code_challenge: Set(code_challenge.to_string()),
        hostname: Set(String::new()),
        created_at: Set(now.fixed_offset()),
        expires_at: Set((now + Duration::seconds(CODE_TTL_SECS)).fixed_offset()),
        consumed_at: Set(None),
        mint: Set(Some(
            json!({ "kind": CODE_KIND, "redirect_uri": redirect_uri }),
        )),
    }
    .insert(db)
    .await
    .map_err(|e| OxyError::DBError(format!("store mobile auth code: {e}")))?;
    Ok(code)
}

/// Redeem `code` with `code_verifier` and the `redirect_uri` it was issued
/// for: the user it signs in. `Ok(None)` for every failure — unknown, spent,
/// expired, another kind of row, a verifier or redirect URI that does not
/// match — which the caller answers identically.
///
/// Spent **first**, in one conditional `UPDATE`, as an `oxyc login` code is:
/// two exchanges cannot both win, and a failed attempt still burns the code.
pub async fn redeem<C: ConnectionTrait>(
    db: &C,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<Option<Uuid>, OxyError> {
    let now = Utc::now();
    let Some(row) = cli_login::spend(db, hash_code(code.trim()), now).await? else {
        return Ok(None);
    };
    Ok(honour(&row, code_verifier, redirect_uri, now))
}

/// Whether a spent row signs its user in: it is a live mobile code, the
/// verifier hashes to its challenge, and the redirect URI is the one it was
/// issued for, exactly.
fn honour(
    row: &cli_auth_codes::Model,
    code_verifier: &str,
    redirect_uri: &str,
    now: DateTime<Utc>,
) -> Option<Uuid> {
    let live = DateTime::<Utc>::from(row.expires_at) > now;
    let verifier_ok = VERIFIER_LEN.contains(&code_verifier.len())
        && challenge_of(code_verifier) == row.code_challenge;
    let bound_to = issued_redirect(row.mint.as_ref())?;
    let redirect_ok = clean_redirect_uri(redirect_uri).is_some_and(|uri| uri == bound_to);
    (live && verifier_ok && redirect_ok).then_some(row.user_id)
}

/// The redirect URI a row's `mint` names, when the row is a mobile code's.
fn issued_redirect(mint: Option<&Value>) -> Option<&str> {
    let mint = mint?;
    if mint.get("kind")?.as_str()? != CODE_KIND {
        return None;
    }
    mint.get("redirect_uri")?.as_str()
}

#[cfg(test)]
#[path = "mobile_login_tests.rs"]
mod tests;
