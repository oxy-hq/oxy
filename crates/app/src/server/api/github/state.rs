use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

const STATE_TTL: Duration = Duration::hours(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Oauth,
    Install,
}

impl Flow {
    fn as_str(&self) -> &'static str {
        match self {
            Flow::Oauth => "oauth",
            Flow::Install => "install",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "oauth" => Some(Flow::Oauth),
            "install" => Some(Flow::Install),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatePayload {
    pub org_id: Uuid,
    pub flow: Flow,
}

#[derive(Debug)]
pub enum StateError {
    MissingSecret,
    Malformed,
    BadHmac,
    Expired,
}

impl From<StateError> for StatusCode {
    fn from(err: StateError) -> Self {
        match err {
            StateError::MissingSecret => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        }
    }
}

fn secret() -> Result<String, StateError> {
    std::env::var("GITHUB_STATE_SECRET").map_err(|_| StateError::MissingSecret)
}

fn sign(data: &str, key: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key");
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn encode_state(payload: &StatePayload) -> Result<String, StateError> {
    encode_state_with_timestamp(payload, Utc::now().timestamp())
}

pub fn encode_state_with_timestamp(payload: &StatePayload, ts: i64) -> Result<String, StateError> {
    let key = secret()?;
    let body = format!("{}:{}:{}", payload.org_id, payload.flow.as_str(), ts);
    let sig = sign(&body, key.as_bytes());
    Ok(format!("{body}:{sig}"))
}

pub fn decode_state(state: &str) -> Result<StatePayload, StateError> {
    let key = secret()?;
    let parts: Vec<&str> = state.split(':').collect();
    if parts.len() != 4 {
        return Err(StateError::Malformed);
    }
    let [org_id_s, flow_s, ts_s, sig_s] = [parts[0], parts[1], parts[2], parts[3]];

    let body = format!("{org_id_s}:{flow_s}:{ts_s}");
    let expected = sign(&body, key.as_bytes());
    if !constant_time_eq::constant_time_eq(sig_s.as_bytes(), expected.as_bytes()) {
        return Err(StateError::BadHmac);
    }

    let ts = ts_s.parse::<i64>().map_err(|_| StateError::Malformed)?;
    let when = DateTime::<Utc>::from_timestamp(ts, 0).ok_or(StateError::Malformed)?;
    if Utc::now().signed_duration_since(when) > STATE_TTL {
        return Err(StateError::Expired);
    }

    Ok(StatePayload {
        org_id: Uuid::parse_str(org_id_s).map_err(|_| StateError::Malformed)?,
        flow: Flow::from_str(flow_s).ok_or(StateError::Malformed)?,
    })
}

#[cfg(test)]
pub(crate) fn sign_for_test(body: &str) -> String {
    sign(body, secret().unwrap().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn env_secret() {
        // SAFETY: tests run single-threaded per file; overriding a process env var is fine here.
        unsafe { std::env::set_var("GITHUB_STATE_SECRET", "test-secret-value-abc") };
    }

    #[test]
    fn encode_then_decode_roundtrips() {
        env_secret();
        let org_id = Uuid::new_v4();
        let state = encode_state(&StatePayload {
            org_id,
            flow: Flow::Oauth,
        })
        .unwrap();

        let decoded = decode_state(&state).unwrap();
        assert_eq!(decoded.org_id, org_id);
        assert!(matches!(decoded.flow, Flow::Oauth));
    }

    #[test]
    fn flow_install_roundtrips() {
        env_secret();
        let payload = StatePayload {
            org_id: Uuid::new_v4(),
            flow: Flow::Install,
        };
        let encoded = encode_state(&payload).unwrap();
        let decoded = decode_state(&encoded).unwrap();
        assert!(matches!(decoded.flow, Flow::Install));
    }

    #[test]
    fn tampered_hmac_rejected() {
        env_secret();
        let org_id = Uuid::new_v4();
        let mut encoded = encode_state(&StatePayload {
            org_id,
            flow: Flow::Oauth,
        })
        .unwrap();
        let last = encoded.pop().unwrap();
        encoded.push(if last == 'a' { 'b' } else { 'a' });
        assert!(decode_state(&encoded).is_err());
    }

    #[test]
    fn expired_state_rejected() {
        env_secret();
        let org_id = Uuid::new_v4();
        let old_ts = (chrono::Utc::now() - chrono::Duration::hours(2)).timestamp();
        let encoded = encode_state_with_timestamp(
            &StatePayload {
                org_id,
                flow: Flow::Oauth,
            },
            old_ts,
        )
        .unwrap();
        assert!(decode_state(&encoded).is_err());
    }

    #[test]
    fn unknown_flow_rejected() {
        env_secret();
        let raw = format!(
            "{}:{}:{}",
            Uuid::new_v4(),
            "unknown",
            chrono::Utc::now().timestamp()
        );
        let hmac = sign_for_test(&raw);
        let tampered = format!("{}:{}", raw, hmac);
        assert!(decode_state(&tampered).is_err());
    }
}
