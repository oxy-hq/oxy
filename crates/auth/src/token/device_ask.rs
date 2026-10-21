//! The device-code flow's two pieces of text (`device_login`): the **user
//! code** a person compares on the page, and the **ask** — the `/cli-auth`
//! parameters a CLI requests a token with. Pure: no database, no clock.

use serde_json::{Map, Value};

/// RFC 8628 §6.1: consonants only, so no word is spelled and no `0`/`O`
/// or `1`/`I` is confused. 20^8 ≈ 2.6 × 10^10 codes.
pub(super) const USER_CODE_ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
pub const USER_CODE_LEN: usize = 8;
/// The `/cli-auth` parameters an ask may carry, each with its length limit.
const ASK_FIELDS: [(&str, usize); 5] = [
    ("kind", 32),
    ("apps", 2048),
    ("hours", 8),
    ("name", 400),
    ("standing", 8),
];
/// The kinds a request may ask for, besides a login (no `kind`).
const ASK_KINDS: [&str; 2] = ["sandbox_agent", "agent"];

/// A fresh user code, stored and compared without its dash.
pub(super) fn new_user_code() -> String {
    let mut code = String::with_capacity(USER_CODE_LEN);
    while code.len() < USER_CODE_LEN {
        // 240 = 12 × 20: drawing below it keeps every letter equally likely.
        let byte = rand::random::<u8>();
        if byte < 240 {
            code.push(USER_CODE_ALPHABET[usize::from(byte) % USER_CODE_ALPHABET.len()] as char);
        }
    }
    code
}

/// `BCDF-GHJK`: how the CLI prints a user code and the page shows it.
pub fn display_user_code(code: &str) -> String {
    let (head, tail) = code.split_at(code.len().min(USER_CODE_LEN / 2));
    format!("{head}-{tail}")
}

/// A user code as typed — any case, with or without the dash or spaces — in
/// its stored form, or `None` when it cannot be one.
pub fn clean_user_code(raw: &str) -> Option<String> {
    let code: String = raw
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let ok = code.len() == USER_CODE_LEN && code.bytes().all(|b| USER_CODE_ALPHABET.contains(&b));
    ok.then_some(code)
}

/// The kind of token an ask is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskedKind {
    /// No `kind`: `oxyc login`.
    Login,
    /// `kind=sandbox_agent` or `kind=agent`: a mint.
    Mint,
}

/// The kind an ask is for. Only a cleaned ask is ever stored.
pub fn asked_kind(ask: &Value) -> AskedKind {
    match ask.get("kind") {
        Some(_) => AskedKind::Mint,
        None => AskedKind::Login,
    }
}

/// An ask as it will be stored, or why it cannot be: an object of the known
/// `/cli-auth` parameters, each a string within its limit and free of control
/// characters, with a known `kind` — and, for a login, no field at all.
/// What the values mean is the page's to read and the approval's to check.
pub fn clean_ask(ask: Option<&Value>) -> Result<Value, String> {
    let Some(ask) = ask.filter(|ask| !ask.is_null()) else {
        return Ok(Value::Object(Map::new()));
    };
    let fields = ask.as_object().ok_or("'ask' must be an object")?;
    for (key, value) in fields {
        let limit = ASK_FIELDS
            .iter()
            .find(|(name, _)| *name == key.as_str())
            .map(|(_, limit)| *limit)
            .ok_or_else(|| format!("'ask' has an unknown field '{key}'"))?;
        let text = value
            .as_str()
            .ok_or_else(|| format!("'ask.{key}' must be a string"))?;
        if text.chars().count() > limit || text.chars().any(char::is_control) {
            return Err(format!(
                "'ask.{key}' is too long or carries a control character"
            ));
        }
    }
    match fields.get("kind").and_then(Value::as_str) {
        Some(kind) if !ASK_KINDS.contains(&kind) => Err(format!("'ask.kind' '{kind}' is unknown")),
        None if !fields.is_empty() => Err("'ask' names no 'kind', so it asks for a login".into()),
        _ => Ok(ask.clone()),
    }
}
