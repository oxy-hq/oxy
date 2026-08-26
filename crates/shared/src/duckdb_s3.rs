//! The one recipe for teaching a DuckDB connection to read `s3://` URLs.
//!
//! Two unrelated features need it and must not drift: the DuckDB *warehouse*
//! connector reads a compile-time S3 mirror of a local-file database
//! (`oxy-compile::duckdb_mirror` → `connector::duckdb::build_s3_mirror_sql`),
//! and the *pre-aggregation* read path reads a rollup Parquet another node
//! built (`agentic_semantic::preagg`). The endpoint handling below is the part
//! worth having in one place — it was learned once, painfully, and getting it
//! wrong fails every read with a hostname error rather than anything that
//! points at S3 configuration.
//!
//! Lives in `oxy-shared` because `agentic-semantic` may not depend on `oxy`.

/// Per-range-request bounds for an `s3://` read.
///
/// **This is the only place an `s3://` read is actually bounded.** DuckDB
/// defaults to 30s with 3 retries, applied *per range request* — and a Parquet
/// scan makes many — so an endpoint that black-holes has no bound worth the
/// name. A Rust-side `tokio::time::timeout` around the call does not supply
/// one: every caller runs the read inside `spawn_blocking`, and dropping that
/// future detaches the task rather than cancelling it, leaving the thread and
/// its DuckDB connection alive while the caller moves on. Some callers (the
/// analytics chat path) have no timeout at all. So the ceiling has to live
/// where the read does.
///
/// The two callers want different numbers, which is why this is a parameter
/// and not a constant: what a timeout COSTS depends on whether the caller has
/// somewhere else to go when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct S3ReadBounds {
    /// Per-HTTP-request ceiling, in milliseconds.
    pub timeout_ms: u32,
    /// Retries per range request (DuckDB's own default is 3, 4x backoff from
    /// 100ms).
    pub retries: u32,
}

impl S3ReadBounds {
    /// For a read whose caller re-runs the query somewhere else if this fails
    /// — the pre-aggregation read path, where every caller falls back to the
    /// warehouse SQL the `Preaggregation` variant carries. A tripped bound
    /// costs a slower answer, so it can be tight: 5s is generous for one range
    /// request against S3 or MinIO, and keeps the tail on an unreachable
    /// endpoint to seconds per request rather than minutes. Two retries still
    /// absorb the transient 5xx S3 hands out, with a third less tail.
    pub const WITH_FALLBACK: Self = Self {
        timeout_ms: 5_000,
        retries: 2,
    };

    /// For a read that IS the answer — the DuckDB warehouse connector's S3
    /// mirror, where a tripped bound is a failed query with nowhere to fall
    /// back to. Still bounded (the whole point above), but generously: a large
    /// mirrored object over a slow link must not start failing queries that
    /// worked before these bounds existed.
    pub const NO_FALLBACK: Self = Self {
        timeout_ms: 20_000,
        retries: 3,
    };
}

/// SQL to run, in order, before any `s3://` read on a connection.
///
/// `secret_name` scopes the secret so two features on one connection can't
/// clobber each other's credentials. It is emitted bare, so it must be a plain
/// identifier — every caller passes a compile-time constant, and the assertion
/// below is there to keep it that way rather than to sanitise user input.
///
/// Credentials come from the pod's own chain (`PROVIDER credential_chain`) —
/// nothing is stored, and nothing is interpolated here that could carry a key.
///
/// `REFRESH true` is load-bearing, not decoration. The chain is resolved
/// EAGERLY at create time, so the secret holds whatever key/secret/session
/// token it produced then — and on EKS (IRSA web identity) or EC2 (IMDS) those
/// are time-boxed to roughly an hour. A caller that issues this once per
/// process (`agentic_semantic::preagg` does, because eager resolution is
/// exactly what makes re-issuing per read expensive) would otherwise start
/// 403ing about an hour into a pod's life and stay there. `REFRESH` tells
/// DuckDB to re-resolve the chain when the credential expires.
///
/// Order matters twice: the `SET`s must follow `LOAD httpfs`, which is what
/// registers those settings, and a caller that skips the extension statements
/// on later calls (`agentic_semantic::preagg` does, since `httpfs` is
/// per-DuckDB-instance) must still run the rest.
pub fn s3_setup_sql(
    secret_name: &str,
    region: Option<&str>,
    endpoint_url: Option<&str>,
    bounds: S3ReadBounds,
) -> Vec<String> {
    let mut stmts = vec![
        "INSTALL httpfs".to_string(),
        "LOAD httpfs".to_string(),
        format!("SET http_timeout = {}", bounds.timeout_ms),
        format!("SET http_retries = {}", bounds.retries),
    ];

    let region = region.unwrap_or("us-east-1");
    debug_assert!(
        !secret_name.is_empty()
            && secret_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "secret name must be a plain identifier, got {secret_name:?}"
    );
    let mut secret = format!(
        "CREATE OR REPLACE SECRET {secret_name} (TYPE s3, PROVIDER credential_chain, \
         REFRESH true, REGION '{}'",
        escape_string(region)
    );
    if let Some(endpoint) = endpoint_url.map(str::trim).filter(|e| !e.is_empty()) {
        // Custom endpoint (MinIO / LocalStack) → path-style addressing. DuckDB's
        // S3 secret ENDPOINT is host[:port] WITHOUT a scheme — it prepends
        // http(s):// itself based on USE_SSL. Callers record the SDK's
        // `AWS_ENDPOINT_URL` verbatim (e.g. `http://localhost:9000`), so strip
        // the scheme here; otherwise DuckDB builds `http://http://localhost:9000`
        // and fails with "Could not resolve hostname", which reads as a missing
        // database rather than a misconfigured endpoint.
        let use_ssl = !endpoint.starts_with("http://");
        let host = endpoint
            .strip_prefix("http://")
            .or_else(|| endpoint.strip_prefix("https://"))
            .unwrap_or(endpoint)
            .trim_end_matches('/');
        secret.push_str(&format!(
            ", ENDPOINT '{}', URL_STYLE 'path', USE_SSL {}",
            escape_string(host),
            use_ssl
        ));
    }
    secret.push(')');
    stmts.push(secret);
    stmts
}

pub fn escape_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `CREATE SECRET`, wherever it landed — the tests care what it says,
    /// not what index it sits at, so adding a statement can't silently make
    /// them assert against the wrong one.
    fn secret(stmts: &[String]) -> &str {
        stmts
            .iter()
            .find(|s| s.starts_with("CREATE OR REPLACE SECRET"))
            .map(String::as_str)
            .unwrap_or_else(|| panic!("no secret statement in {stmts:?}"))
    }

    #[test]
    fn a_plain_aws_secret_carries_no_endpoint() {
        let stmts = s3_setup_sql(
            "preagg_s3",
            Some("eu-west-1"),
            None,
            S3ReadBounds::WITH_FALLBACK,
        );
        assert_eq!(stmts[0], "INSTALL httpfs");
        assert_eq!(stmts[1], "LOAD httpfs");
        let secret = secret(&stmts);
        assert!(secret.contains("REGION 'eu-west-1'"));
        assert!(secret.contains("PROVIDER credential_chain"));
        assert!(
            !secret.contains("ENDPOINT"),
            "real AWS must not get an ENDPOINT override: {secret}"
        );
    }

    /// The only bound on an `s3://` read that survives the caller dropping its
    /// future — every caller reads inside `spawn_blocking`, where a dropped
    /// future detaches the task instead of cancelling it. Without these, an
    /// unreachable endpoint runs at DuckDB's 30s-times-4 default per range
    /// request, for as many requests as the scan makes.
    #[test]
    fn every_s3_read_is_bounded_after_httpfs_loads() {
        for bounds in [S3ReadBounds::WITH_FALLBACK, S3ReadBounds::NO_FALLBACK] {
            let stmts = s3_setup_sql("preagg_s3", None, None, bounds);
            let timeout = stmts
                .iter()
                .position(|s| s == &format!("SET http_timeout = {}", bounds.timeout_ms))
                .unwrap_or_else(|| panic!("no http_timeout in {stmts:?}"));
            let retries = stmts
                .iter()
                .position(|s| s == &format!("SET http_retries = {}", bounds.retries))
                .unwrap_or_else(|| panic!("no http_retries in {stmts:?}"));
            let load = stmts.iter().position(|s| s == "LOAD httpfs").expect("load");
            // `httpfs` registers these settings, so a SET before LOAD errors out —
            // and the caller would surface it as "could not prepare DuckDB".
            assert!(timeout > load, "http_timeout must follow LOAD: {stmts:?}");
            assert!(retries > load, "http_retries must follow LOAD: {stmts:?}");
        }
    }

    /// Both presets must stay strictly below DuckDB's own 30s default —
    /// a "bound" at or above the default is not a bound.
    #[test]
    fn neither_preset_is_looser_than_duckdbs_default() {
        for bounds in [S3ReadBounds::WITH_FALLBACK, S3ReadBounds::NO_FALLBACK] {
            assert!(
                bounds.timeout_ms < 30_000,
                "{bounds:?} does not bound anything"
            );
        }
        assert!(
            S3ReadBounds::WITH_FALLBACK.timeout_ms < S3ReadBounds::NO_FALLBACK.timeout_ms,
            "the path that can fall back is the one that may give up sooner"
        );
    }

    /// A cached secret that cannot re-resolve its chain is a blob tier that
    /// goes dark an hour into a pod's life — see the `REFRESH` paragraph on
    /// `s3_setup_sql`. Pinned here because the caller that caches it cannot
    /// see this statement.
    #[test]
    fn the_credential_chain_secret_refreshes_itself() {
        let stmts = s3_setup_sql("preagg_s3", None, None, S3ReadBounds::WITH_FALLBACK);
        let secret = secret(&stmts);
        assert!(secret.contains("PROVIDER credential_chain"), "{secret}");
        assert!(
            secret.contains("REFRESH true"),
            "an eagerly-resolved credential_chain secret must be allowed to re-resolve: {secret}"
        );
    }

    /// The scheme-stripping bug this helper exists to hold in one place:
    /// DuckDB prepends the scheme itself, so passing `http://host:9000`
    /// through produces `http://http://host:9000`.
    #[test]
    fn a_custom_endpoint_is_stripped_to_host_and_switches_off_ssl() {
        let stmts = s3_setup_sql(
            "preagg_s3",
            None,
            Some("http://localhost:9000"),
            S3ReadBounds::WITH_FALLBACK,
        );
        let http = secret(&stmts);
        assert!(http.contains("ENDPOINT 'localhost:9000'"), "{http}");
        assert!(http.contains("USE_SSL false"), "{http}");
        assert!(http.contains("URL_STYLE 'path'"), "{http}");

        let stmts = s3_setup_sql(
            "preagg_s3",
            None,
            Some("https://minio.internal"),
            S3ReadBounds::WITH_FALLBACK,
        );
        let https = secret(&stmts);
        assert!(https.contains("ENDPOINT 'minio.internal'"), "{https}");
        assert!(https.contains("USE_SSL true"), "{https}");
    }

    #[test]
    fn a_trailing_slash_is_trimmed_off_the_host() {
        let stmts = s3_setup_sql(
            "s",
            None,
            Some("http://localhost:9000/"),
            S3ReadBounds::WITH_FALLBACK,
        );
        let secret = secret(&stmts);
        assert!(secret.contains("ENDPOINT 'localhost:9000'"), "{secret}");
    }

    #[test]
    fn an_empty_endpoint_is_treated_as_unset() {
        let stmts = s3_setup_sql("preagg_s3", None, Some("   "), S3ReadBounds::WITH_FALLBACK);
        let secret = secret(&stmts);
        assert!(!secret.contains("ENDPOINT"), "{secret}");
    }

    #[test]
    fn a_quote_cannot_break_out_of_a_single_quoted_literal() {
        assert_eq!(escape_string("b'ucket"), "b''ucket");
    }
}
