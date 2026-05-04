//! Stripe webhook signature verification against raw request bytes.

use crate::errors::BillingError;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

const TOLERANCE_SECS: i64 = 300;

pub fn verify_signature(
    body: &[u8],
    header: &str,
    secret: &str,
    now_unix: i64,
) -> Result<(), BillingError> {
    let mut t: Option<i64> = None;
    let mut sigs: Vec<&str> = Vec::new();
    for part in header.split(',') {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| BillingError::MalformedEvent("bad sig header".into()))?;
        match k {
            "t" => t = v.parse().ok(),
            "v1" => sigs.push(v),
            _ => {}
        }
    }
    let t = t.ok_or_else(|| BillingError::MalformedEvent("no t=".into()))?;
    if (now_unix - t).abs() > TOLERANCE_SECS {
        return Err(BillingError::StaleWebhook);
    }

    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .map_err(|_| BillingError::InvalidSignature)?;
    mac.update(format!("{t}.").as_bytes());
    mac.update(body);
    let expected = mac.finalize().into_bytes();

    for s in sigs {
        if let Ok(got) = hex::decode(s) {
            if got.len() == expected.len()
                && got
                    .iter()
                    .zip(expected.iter())
                    .fold(0u8, |a, (x, y)| a | (x ^ y))
                    == 0
            {
                return Ok(());
            }
        }
    }
    Err(BillingError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, t: i64, body: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.").as_bytes());
        mac.update(body);
        format!("t={t},v1={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verify_ok_when_signature_matches() {
        let body = br#"{"id":"evt_1"}"#;
        let now = chrono::Utc::now().timestamp();
        let sig = sign("whsec_test", now, body);
        assert!(verify_signature(body, &sig, "whsec_test", now).is_ok());
    }

    #[test]
    fn reject_wrong_secret() {
        let body = br#"{}"#;
        let now = chrono::Utc::now().timestamp();
        let sig = sign("whsec_wrong", now, body);
        assert!(matches!(
            verify_signature(body, &sig, "whsec_right", now).unwrap_err(),
            BillingError::InvalidSignature
        ));
    }

    #[test]
    fn reject_tampered_body() {
        let body = br#"{"id":"evt_1"}"#;
        let now = chrono::Utc::now().timestamp();
        let sig = sign("whsec_test", now, body);
        let tampered = br#"{"id":"evt_2"}"#;
        assert!(matches!(
            verify_signature(tampered, &sig, "whsec_test", now).unwrap_err(),
            BillingError::InvalidSignature
        ));
    }

    #[test]
    fn reject_stale_timestamp() {
        let body = br#"{}"#;
        let stale = chrono::Utc::now().timestamp() - 600; // 10 min old
        let sig = sign("whsec_test", stale, body);
        assert!(matches!(
            verify_signature(body, &sig, "whsec_test", chrono::Utc::now().timestamp()).unwrap_err(),
            BillingError::StaleWebhook
        ));
    }

    #[test]
    fn reject_malformed_header() {
        assert!(verify_signature(b"{}", "garbage", "whsec_test", 0).is_err());
    }

    #[test]
    fn reject_missing_timestamp() {
        let body = br#"{}"#;
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(b"whsec_test").unwrap();
        mac.update(body);
        let sig_only = format!("v1={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_signature(body, &sig_only, "whsec_test", 0).is_err());
    }
}
