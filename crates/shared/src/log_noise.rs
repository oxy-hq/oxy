//! The framework crates whose chatter is held at `warn` on every log filter
//! the platform builds — stderr, OTLP export, and the product observability
//! store — so `debug` means *oxy's* debug.
//!
//! `EnvFilter` matches a directive's target as a string **prefix** and picks
//! the most specific match, so `opentelemetry=warn` covers `opentelemetry_sdk`,
//! `opentelemetry-otlp` and `opentelemetry-http`, `hyper=warn` covers
//! `hyper_rustls`, `tower=warn` covers `tower_http`, and the four `aws_*`
//! families cover every AWS crate. Keep entries at the prefix that names the
//! family; a longer twin of an existing entry is inert.
//!
//! Measured on oxy-dev at debug (2026-09-08): the OpenTelemetry SDK narrating
//! its own exports was ~9k lines an hour and the AWS SDK config/credential
//! chain ~2.5k — none of it about oxy, all of it in the way. The product store,
//! which defaults to `debug`, was taking the same crates' *spans* as
//! tenant-visible rows (~280k a day). `clickhouse` is on the list for that store's
//! sake: its own inserts go through the `clickhouse` crate, whose `insert`
//! spans the store would otherwise capture and queue for the next insert.
//!
//! What is deliberately **not** here: `custom_app_function=warn`, the privacy
//! boundary on a tenant's `ctx.log()` lines. That belongs to the platform-side
//! filters only (`oxy_telemetry::NOISY_CRATE_DIRECTIVES`); the product store
//! keeps every such line behind the app-admin gate.

/// The list as a literal, so a dependent can `concat!` onto it.
#[macro_export]
macro_rules! framework_noise_directives {
    () => {
        "tower=warn,h2=warn,hyper=warn,reqwest=warn,sqlx=warn,sea_orm=warn,\
         tonic=warn,rustls=warn,tokio_postgres=warn,tungstenite=warn,tokio_tungstenite=warn,\
         deser_incomplete=off,opentelemetry=warn,aws_config=warn,aws_runtime=warn,\
         aws_smithy=warn,aws_sdk=warn,clickhouse=warn"
    };
}

/// See the module doc.
pub const FRAMEWORK_NOISE_DIRECTIVES: &str = framework_noise_directives!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_a_family_prefix_with_no_longer_twin() {
        let targets: Vec<&str> = FRAMEWORK_NOISE_DIRECTIVES
            .split(',')
            .map(|d| d.trim().split('=').next().unwrap())
            .collect();
        for (i, a) in targets.iter().enumerate() {
            for (j, b) in targets.iter().enumerate() {
                if i != j {
                    assert!(!b.starts_with(a), "{b} is an inert twin of {a}");
                }
            }
        }
        assert!(!FRAMEWORK_NOISE_DIRECTIVES.contains("custom_app_function"));
    }
}
