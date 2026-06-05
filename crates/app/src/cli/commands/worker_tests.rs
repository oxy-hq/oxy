//! Unit tests for the `oxy worker` subcommand.
//!
//! Co-located via `#[path = "worker_tests.rs"] mod tests;` from `worker.rs`
//! to keep `worker.rs` under the 400-line file limit.

use super::*;
use clap::Parser;
use std::sync::Mutex;

/// Serializes env-mutating tests within this binary. Nextest runs tests
/// in parallel by default, so without this mutex two env writes can race
/// and break each other's setup/teardown.
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn worker_args_parse_with_defaults() {
    let args = WorkerArgs::parse_from(["oxy"]);
    assert!(!args.enterprise);
    assert!(!args.skip_migrations);
    assert_eq!(args.recovery_interval_secs, None);
    assert_eq!(args.health_port, None);
}

#[test]
fn worker_args_parse_explicit_flags() {
    let args = WorkerArgs::parse_from([
        "oxy",
        "--enterprise",
        "--skip-migrations",
        "--recovery-interval-secs",
        "5",
        "--health-port",
        "9090",
    ]);
    assert!(args.enterprise);
    assert!(args.skip_migrations);
    assert_eq!(args.recovery_interval_secs, Some(5));
    assert_eq!(args.health_port, Some(9090));
}

/// All env-mutating cases bundled together so the lock is held across the
/// full sequence — each `match prev` arm restores prior state before the
/// next case runs.
#[test]
fn recovery_interval_resolution() {
    let _g = ENV_LOCK.lock().unwrap();
    let prev = std::env::var(RECOVERY_INTERVAL_ENV).ok();

    // SAFETY: env mutation is serialized via ENV_LOCK above.
    unsafe { std::env::remove_var(RECOVERY_INTERVAL_ENV) };
    assert_eq!(
        resolve_recovery_interval(None).as_secs(),
        DEFAULT_RECOVERY_INTERVAL_SECS,
        "default when neither CLI nor env set"
    );

    unsafe { std::env::set_var(RECOVERY_INTERVAL_ENV, "15") };
    assert_eq!(
        resolve_recovery_interval(None).as_secs(),
        15,
        "env wins when CLI is absent"
    );

    unsafe { std::env::set_var(RECOVERY_INTERVAL_ENV, "9999") };
    assert_eq!(
        resolve_recovery_interval(Some(7)).as_secs(),
        7,
        "CLI wins over env"
    );

    match prev {
        Some(v) => unsafe { std::env::set_var(RECOVERY_INTERVAL_ENV, v) },
        None => unsafe { std::env::remove_var(RECOVERY_INTERVAL_ENV) },
    }
}

#[test]
fn max_inflight_resolution() {
    let _g = ENV_LOCK.lock().unwrap();
    let prev = std::env::var(agentic_runtime::worker::MAX_INFLIGHT_ENV).ok();

    unsafe { std::env::remove_var(agentic_runtime::worker::MAX_INFLIGHT_ENV) };
    assert_eq!(
        read_max_inflight(),
        agentic_runtime::worker::DEFAULT_MAX_INFLIGHT
    );

    unsafe { std::env::set_var(agentic_runtime::worker::MAX_INFLIGHT_ENV, "64") };
    assert_eq!(read_max_inflight(), 64);

    match prev {
        Some(v) => unsafe { std::env::set_var(agentic_runtime::worker::MAX_INFLIGHT_ENV, v) },
        None => unsafe { std::env::remove_var(agentic_runtime::worker::MAX_INFLIGHT_ENV) },
    }
}

#[test]
fn mask_db_url_redacts_password() {
    let masked = mask_db_url_str("postgresql://user:supersecret@db.example.com:5432/oxy");
    assert!(!masked.contains("supersecret"), "password leaked: {masked}");
    assert!(masked.contains("db.example.com"));
}

#[test]
fn mask_db_url_handles_no_password() {
    let masked = mask_db_url_str("postgresql://db.example.com:5432/oxy");
    assert!(masked.contains("db.example.com"));
}

#[test]
fn mask_db_url_handles_invalid() {
    let masked = mask_db_url_str("not a url");
    assert!(masked.contains("invalid"));
}

#[test]
fn resolve_health_port_prefers_cli_over_env() {
    let _g = ENV_LOCK.lock().unwrap();
    let prev = std::env::var(HEALTH_PORT_ENV).ok();

    unsafe { std::env::remove_var(HEALTH_PORT_ENV) };
    assert_eq!(resolve_health_port(None), None, "no CLI, no env -> None");

    unsafe { std::env::set_var(HEALTH_PORT_ENV, "8081") };
    assert_eq!(
        resolve_health_port(None),
        Some(8081),
        "env wins when CLI absent"
    );

    unsafe { std::env::set_var(HEALTH_PORT_ENV, "9999") };
    assert_eq!(
        resolve_health_port(Some(7777)),
        Some(7777),
        "CLI wins over env"
    );

    unsafe { std::env::set_var(HEALTH_PORT_ENV, "bogus") };
    assert_eq!(
        resolve_health_port(None),
        None,
        "unparsable env -> None (graceful)"
    );

    match prev {
        Some(v) => unsafe { std::env::set_var(HEALTH_PORT_ENV, v) },
        None => unsafe { std::env::remove_var(HEALTH_PORT_ENV) },
    }
}

#[test]
fn worker_id_is_stable_and_includes_pid() {
    let id = compute_worker_id();
    let pid = std::process::id().to_string();
    assert!(
        id.ends_with(&format!("@{pid}")),
        "worker_id should end with @pid: {id}"
    );
    let id2 = compute_worker_id();
    assert_eq!(id, id2, "worker_id should be deterministic per process");
}

#[test]
fn require_database_url_errors_when_unset() {
    let _g = ENV_LOCK.lock().unwrap();
    let prev = std::env::var("OXY_DATABASE_URL").ok();
    unsafe {
        std::env::remove_var("OXY_DATABASE_URL");
        std::env::remove_var("OXY_DATABASE_AUTH_MODE");
    }

    let err = require_database_url().expect_err("expected error when unset");
    let msg = err.to_string();
    assert!(
        msg.contains("OXY_DATABASE_URL"),
        "unexpected error message: {msg}"
    );

    match prev {
        Some(v) => unsafe { std::env::set_var("OXY_DATABASE_URL", v) },
        None => {}
    }
}

fn clear_iam_env() {
    unsafe {
        for var in [
            "OXY_DATABASE_URL",
            "OXY_DATABASE_AUTH_MODE",
            "OXY_DATABASE_HOST",
            "OXY_DATABASE_PORT",
            "OXY_DATABASE_NAME",
            "OXY_DATABASE_USER",
            "OXY_DATABASE_REGION",
            "OXY_DATABASE_SSL_MODE",
        ] {
            std::env::remove_var(var);
        }
    }
}

fn set_iam_env_minimal() {
    unsafe {
        std::env::set_var("OXY_DATABASE_AUTH_MODE", "iam");
        std::env::set_var("OXY_DATABASE_HOST", "rds.example.com");
        std::env::set_var("OXY_DATABASE_NAME", "oxydb");
        std::env::set_var("OXY_DATABASE_USER", "oxy_app");
        std::env::set_var("OXY_DATABASE_REGION", "us-east-1");
    }
}

/// IAM mode with all required vars set — must succeed even with no URL.
#[test]
fn require_database_url_iam_mode_ok_with_all_vars() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_iam_env();
    set_iam_env_minimal();

    assert!(
        require_database_url().is_ok(),
        "IAM mode with all vars set should succeed"
    );

    clear_iam_env();
}

/// IAM mode with a missing required var — must fail with a clear message.
#[test]
fn require_database_url_iam_mode_err_when_var_missing() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_iam_env();
    set_iam_env_minimal();
    unsafe { std::env::remove_var("OXY_DATABASE_REGION") };

    let err = require_database_url().expect_err("should fail with missing REGION");
    let msg = err.to_string();
    assert!(
        msg.contains("OXY_DATABASE_REGION"),
        "error should name the missing var: {msg}"
    );

    clear_iam_env();
}

/// IAM mode with a malformed port — IamConfig::from_env parses it and must
/// fail before the connection is attempted.
#[test]
fn require_database_url_iam_mode_err_malformed_port() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_iam_env();
    set_iam_env_minimal();
    unsafe { std::env::set_var("OXY_DATABASE_PORT", "notaport") };

    assert!(
        require_database_url().is_err(),
        "malformed OXY_DATABASE_PORT should be rejected"
    );

    clear_iam_env();
}

/// IAM mode with an unsupported ssl-mode value — same fail-fast guarantee.
#[test]
fn require_database_url_iam_mode_err_bad_ssl_mode() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_iam_env();
    set_iam_env_minimal();
    unsafe { std::env::set_var("OXY_DATABASE_SSL_MODE", "bogus") };

    assert!(
        require_database_url().is_err(),
        "unsupported OXY_DATABASE_SSL_MODE should be rejected"
    );

    clear_iam_env();
}

/// An unrecognised auth mode (e.g. a typo) must be rejected, not silently
/// treated as password mode.
#[test]
fn require_database_url_rejects_unknown_auth_mode() {
    let _g = ENV_LOCK.lock().unwrap();
    clear_iam_env();
    unsafe { std::env::set_var("OXY_DATABASE_AUTH_MODE", "kerberos") };

    assert!(
        require_database_url().is_err(),
        "unknown OXY_DATABASE_AUTH_MODE should be rejected"
    );

    clear_iam_env();
}
