//! RDS IAM auth token generation.
//!
//! Generates a short-lived SigV4 pre-signed URL that RDS accepts as the
//! `password` field of the Postgres startup packet. Token TTL is 15 minutes;
//! we refresh every ~10 minutes in the client-side loop to stay well inside
//! the window.
//!
//! This is the pattern documented in the official AWS SDK for Rust RDS Lambda
//! example — using `aws-sigv4` directly because `aws-sdk-rds` still has no
//! built-in auth-token generator (awslabs/aws-sdk-rust#792, #951).

use std::time::{Duration, SystemTime};

use aws_config::BehaviorVersion;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SignatureLocation, SigningSettings, sign},
    sign::v4,
};
use oxy_shared::errors::OxyError;

use super::auth_mode::IamConfig;

/// RDS IAM auth tokens are valid for 15 minutes from the `time` used to sign.
const TOKEN_TTL_SECONDS: u64 = 900;

pub async fn generate_auth_token(config: &IamConfig) -> Result<String, OxyError> {
    let sdk_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

    let credentials = sdk_config
        .credentials_provider()
        .ok_or_else(|| {
            OxyError::Database(
                "No AWS credentials provider configured for RDS IAM auth. Ensure the pod has \
                 an IAM role (Pod Identity / IRSA) or AWS credentials in the environment."
                    .to_string(),
            )
        })?
        .provide_credentials()
        .await
        .map_err(|e| {
            OxyError::Database(format!(
                "Failed to load AWS credentials for RDS IAM auth: {e}"
            ))
        })?;

    let identity = credentials.into();

    // Prefer the SDK-resolved region (which picks up IMDS / env / config) but
    // fall back to the explicit env-configured region so misconfigured pods
    // surface a clear error instead of a silent cross-region signature.
    let region = sdk_config
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| config.region.clone());

    let mut signing_settings = SigningSettings::default();
    signing_settings.expires_in = Some(Duration::from_secs(TOKEN_TTL_SECONDS));
    signing_settings.signature_location = SignatureLocation::QueryParams;

    let signing_params = v4::SigningParams::builder()
        .identity(&identity)
        .region(&region)
        .name("rds-db")
        .time(SystemTime::now())
        .settings(signing_settings)
        .build()
        .map_err(|e| OxyError::Database(format!("Failed to build SigV4 signing params: {e}")))?;

    // Build the request URL through `url::Url` so that `Action` and `DBUser`
    // are properly percent-encoded at construction time. RDS usernames are
    // typically alphanumeric + underscore so this is rarely load-bearing,
    // but relying on that invariant here would be fragile — any space or
    // reserved char would otherwise corrupt the query string and either
    // break signing or produce a token RDS rejects.
    let mut url = url::Url::parse(&format!(
        "https://{host}:{port}/",
        host = config.host,
        port = config.port
    ))
    .map_err(|e| OxyError::Database(format!("Failed to construct RDS auth URL: {e}")))?;
    url.query_pairs_mut()
        .append_pair("Action", "connect")
        .append_pair("DBUser", &config.user);
    let url_str = url.to_string();

    let signable_request = SignableRequest::new(
        "GET",
        &url_str,
        std::iter::empty(),
        SignableBody::Bytes(&[]),
    )
    .map_err(|e| OxyError::Database(format!("Failed to construct SigV4 signable request: {e}")))?;

    let (signing_instructions, _sig) = sign(signable_request, &signing_params.into())
        .map_err(|e| OxyError::Database(format!("SigV4 signing failed: {e}")))?
        .into_parts();

    for (name, value) in signing_instructions.params() {
        url.query_pairs_mut().append_pair(name, value);
    }

    // RDS expects the token as the URL *without* the `https://` scheme prefix.
    Ok(url.as_str().trim_start_matches("https://").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::auth_mode::SslMode;
    use serial_test::serial;

    // The token generator calls `aws_config::load_defaults`, which consults the
    // AWS credential chain. Setting env creds short-circuits IMDS/config-file
    // lookups so these tests stay hermetic and don't touch the network.

    fn test_config() -> IamConfig {
        IamConfig {
            host: "oxy-prod-postgres.example.us-west-2.rds.amazonaws.com".to_string(),
            port: 5432,
            database: "oxydb".to_string(),
            user: "oxy_app".to_string(),
            region: "us-west-2".to_string(),
            ssl_mode: SslMode::Require,
        }
    }

    #[tokio::test]
    #[serial]
    async fn token_has_expected_shape() {
        let cfg = test_config();
        let token = with_fake_aws_env_async(|| async {
            generate_auth_token(&cfg).await.expect("token generated")
        })
        .await;

        // No scheme prefix (RDS wants the URL tail only).
        assert!(
            !token.starts_with("https://"),
            "token should not start with https://, got {token}"
        );
        // Must begin with `host:port/?Action=connect&DBUser=<user>`.
        assert!(
            token.starts_with(
                "oxy-prod-postgres.example.us-west-2.rds.amazonaws.com:5432/?Action=connect&DBUser=oxy_app"
            ),
            "unexpected token prefix: {token}"
        );
        // SigV4 required params.
        assert!(token.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(token.contains("X-Amz-Expires=900"));
        assert!(token.contains("X-Amz-Signature="));
        assert!(token.contains("X-Amz-Date="));
        assert!(token.contains("X-Amz-SignedHeaders="));
    }

    #[tokio::test]
    #[serial]
    async fn token_scopes_credential_to_rds_db_service_and_region() {
        let cfg = test_config();
        let token = with_fake_aws_env_async(|| async {
            generate_auth_token(&cfg).await.expect("token generated")
        })
        .await;

        // The credential scope must encode `<region>/rds-db/aws4_request`.
        // Slashes are URL-encoded as %2F in the query string.
        assert!(
            token.contains("us-west-2%2Frds-db%2Faws4_request"),
            "credential scope missing region/rds-db: {token}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn token_encodes_custom_user() {
        let mut cfg = test_config();
        cfg.user = "oxy_readonly".to_string();
        let token = with_fake_aws_env_async(|| async {
            generate_auth_token(&cfg).await.expect("token generated")
        })
        .await;
        assert!(
            token.contains("DBUser=oxy_readonly"),
            "token did not include custom user: {token}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn token_percent_encodes_usernames_with_special_chars() {
        // RDS usernames are normally alphanumeric + underscore, but the URL
        // builder must defend against reserved URL chars regardless so that a
        // misconfigured `OXY_DATABASE_USER` produces a well-formed (if
        // RDS-rejected) token rather than a silently-corrupted one.
        let mut cfg = test_config();
        cfg.user = "foo bar&baz".to_string();
        let token = with_fake_aws_env_async(|| async {
            generate_auth_token(&cfg).await.expect("token generated")
        })
        .await;
        // `url` crate percent-encodes spaces as `+` in application/x-www-form-
        // urlencoded form (the query-pair default) and `&` as `%26`.
        assert!(
            token.contains("DBUser=foo+bar%26baz") || token.contains("DBUser=foo%20bar%26baz"),
            "special chars were not URL-encoded: {token}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn token_fails_cleanly_when_no_credentials_available() {
        // Explicitly clear all AWS env so the credential chain has nothing to
        // resolve. On CI runners without IMDS or ~/.aws config this surfaces as
        // an error; the important property is that we don't panic and the
        // message is actionable.
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
            std::env::remove_var("AWS_SESSION_TOKEN");
            std::env::remove_var("AWS_PROFILE");
            // Keep region set so we don't fail for a different reason.
            std::env::set_var("AWS_REGION", "us-west-2");
            // Point the shared config loader at an empty dir to be sure.
            std::env::set_var("AWS_CONFIG_FILE", "/dev/null");
            std::env::set_var("AWS_SHARED_CREDENTIALS_FILE", "/dev/null");
            // Disable IMDS to avoid hanging on a metadata-service timeout when
            // the test runner happens to be on EC2.
            std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
        }
        let result = generate_auth_token(&test_config()).await;
        unsafe {
            std::env::remove_var("AWS_REGION");
            std::env::remove_var("AWS_CONFIG_FILE");
            std::env::remove_var("AWS_SHARED_CREDENTIALS_FILE");
            std::env::remove_var("AWS_EC2_METADATA_DISABLED");
        }
        assert!(
            result.is_err(),
            "expected credential resolution to fail, got {result:?}"
        );
    }

    // Env vars are process-global, so tests using this helper must stay serial.
    async fn with_fake_aws_env_async<F, Fut, T>(f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE");
            std::env::set_var(
                "AWS_SECRET_ACCESS_KEY",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            );
            std::env::set_var("AWS_REGION", "us-west-2");
            std::env::remove_var("AWS_PROFILE");
            std::env::remove_var("AWS_SESSION_TOKEN");
            std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
        }
        let out = f().await;
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
            std::env::remove_var("AWS_SECRET_ACCESS_KEY");
            std::env::remove_var("AWS_REGION");
            std::env::remove_var("AWS_EC2_METADATA_DISABLED");
        }
        out
    }
}
