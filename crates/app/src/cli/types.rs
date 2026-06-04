//! CLI argument types for Oxy commands

use clap::Parser;

/// Arguments for the `oxy serve` command (web server only, no Docker)
#[derive(Parser, Debug, Clone)]
pub struct ServeArgs {
    /// Port number for the web application server
    ///
    /// Specify which port to bind the Oxy web interface.
    /// Default is 3000 if not specified.
    #[clap(long, default_value_t = 3000)]
    pub port: u16,
    /// Host address to bind the web application server
    ///
    /// Specify which host address to bind the Oxy web interface.
    /// Default is 0.0.0.0 to listen on all interfaces.
    #[clap(long, default_value = "0.0.0.0")]
    pub host: String,
    /// Force HTTP/2 only mode (disable HTTP/1.1)
    ///
    /// When enabled, the server will only accept HTTP/2 connections over TLS.
    /// HTTP/1.1 requests will be rejected. Default supports both protocols.
    #[clap(long, default_value_t = false)]
    pub http2_only: bool,
    /// TLS certificate file for HTTPS (local development)
    #[clap(long, default_value = "localhost+2.pem")]
    pub tls_cert: String,
    /// TLS private key file for HTTPS (local development)
    #[clap(long, default_value = "localhost+2-key.pem")]
    pub tls_key: String,

    /// Port for the internal API server (no authentication required)
    ///
    /// The internal port serves the same API routes without authentication.
    /// Binds to 127.0.0.1 by default for security. Set to 0 to disable.
    #[clap(long, default_value_t = 3001)]
    pub internal_port: u16,

    /// Host address to bind the internal API server
    ///
    /// Default is 127.0.0.1 (localhost only) for security since the internal
    /// port has no authentication. Use 0.0.0.0 for Docker/container deployments
    /// where the port needs to be accessible within the container network.
    #[clap(long, default_value = "127.0.0.1")]
    pub internal_host: String,

    /// Enable enterprise features (observability, analytics)
    #[clap(long, default_value_t = false)]
    pub enterprise: bool,

    /// Run in local mode: single workspace rooted at the current directory,
    /// no orgs, guest authentication. Ignores configured auth provider env vars.
    ///
    /// WARNING: local mode disables authentication entirely. Do not expose
    /// a `--local` instance on a non-loopback interface without a reverse
    /// proxy that restricts access.
    #[clap(long, default_value_t = false)]
    pub local: bool,

    /// Disable in-process agentic workers: HTTP only, no startup recovery and
    /// no periodic global driver loop.
    ///
    /// Use when running a separate `oxy worker` fleet — the workers handle all
    /// task execution, recovery, and scheduler ticks; `oxy serve` only accepts
    /// HTTP requests and writes new tasks to the queue.
    ///
    /// Also honored via the `OXY_DISABLE_INPROCESS_WORKERS` env var; the CLI
    /// flag wins if both are set. Default OFF — existing single-process
    /// deployments are unchanged.
    #[clap(long, default_value_t = false)]
    pub no_workers: bool,
}

impl ServeArgs {
    /// True when in-process worker plumbing (startup recovery, periodic
    /// global driver loop) should be skipped because a separate `oxy worker`
    /// fleet handles execution.
    ///
    /// CLI flag (`--no-workers`) wins; falls back to the
    /// `OXY_DISABLE_INPROCESS_WORKERS` env var (`1`/`true`/`yes`/`on`).
    pub fn workers_disabled(&self) -> bool {
        if self.no_workers {
            return true;
        }
        std::env::var("OXY_DISABLE_INPROCESS_WORKERS")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
    }
}

/// Arguments for the `oxy start` command (Docker containers + web server)
#[derive(Parser, Debug)]
pub struct StartArgs {
    /// Server configuration options (includes --enterprise flag)
    #[clap(flatten)]
    pub serve: ServeArgs,

    /// Clean start: remove existing Docker containers and volumes before starting
    ///
    /// When enabled, removes all Oxy-managed Docker containers and their
    /// associated volumes to start with a fresh state. This is useful for
    /// troubleshooting or resetting the local environment.
    #[clap(long, default_value_t = false)]
    pub clean: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn local_flag_defaults_to_false() {
        let args = ServeArgs::parse_from(["oxy"]);
        assert!(!args.local);
    }

    #[test]
    fn local_flag_is_parsed() {
        let args = ServeArgs::parse_from(["oxy", "--local"]);
        assert!(args.local);
    }

    #[test]
    fn no_workers_defaults_to_false() {
        let args = ServeArgs::parse_from(["oxy"]);
        assert!(!args.no_workers);
    }

    #[test]
    fn no_workers_flag_is_parsed() {
        let args = ServeArgs::parse_from(["oxy", "--no-workers"]);
        assert!(args.no_workers);
        assert!(args.workers_disabled());
    }

    #[test]
    fn workers_disabled_falls_back_to_env() {
        // Lib-binary tests run intra-binary in parallel under nextest, so
        // any test that mutates env state needs to serialize on a shared
        // mutex. Today only this test touches OXY_DISABLE_INPROCESS_WORKERS,
        // but the lock is here so a future test added to this binary
        // doesn't race against this one. Matches the ENV_LOCK pattern in
        // crates/app/src/cli/commands/worker_tests.rs.
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("OXY_DISABLE_INPROCESS_WORKERS").ok();
        // SAFETY: ENV_LOCK serializes env mutation within this binary.
        unsafe { std::env::set_var("OXY_DISABLE_INPROCESS_WORKERS", "1") };
        let args = ServeArgs::parse_from(["oxy"]);
        assert!(args.workers_disabled());
        // Restore prior env state to avoid leaking across tests.
        match prev {
            Some(v) => unsafe { std::env::set_var("OXY_DISABLE_INPROCESS_WORKERS", v) },
            None => unsafe { std::env::remove_var("OXY_DISABLE_INPROCESS_WORKERS") },
        }
    }
}
