//! POC demo: provision a per-org OLTP database on the local Postgres and fill
//! it with the restaurant example, then print how to query it.
//!
//! ```bash
//! # Postgres must be running — `oxy start` gives you one.
//! export OXY_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
//!
//! cargo run -p oxy-oltp --example seed_local
//! cargo run -p oxy-oltp --example seed_local -- --reset
//! cargo run -p oxy-oltp --example seed_local -- --reset --expose-app
//! ```
//!
//! `OXY_DATABASE_URL` must point at a **superuser** — the demo creates
//! databases and roles. That is the one place this diverges from production,
//! where the provider API does it.

use oxy_oltp::local_seed::seed_local_demo;
use oxy_oltp::provider::LocalProvider;
use uuid::Uuid;

/// Fixed org id so re-runs hit the same database. Mirrors airhouse's
/// nil-UUID local org convention, offset so the two are distinguishable in
/// `pg_database`.
const DEMO_ORG_ID: Uuid = Uuid::from_u128(0x0177_0000_0000_0000_0000_0000_0000_0001);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let reset = args.iter().any(|a| a == "--reset");
    let expose_app = args.iter().any(|a| a == "--expose-app");

    let provider = LocalProvider::from_env()?;
    println!("→ provisioning per-org OLTP database on the local cluster…");

    let seeded = match seed_local_demo(&provider, DEMO_ORG_ID, reset, expose_app).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("\n✗ {e}\n");
            eprintln!("  If it is already seeded, re-run with --reset to rebuild it.");
            std::process::exit(1);
        }
    };

    println!("\n✓ database {} on {}\n", seeded.database, seeded.host);

    println!("  writers");
    for w in &seeded.writers {
        let vis = if w.analytics_visible {
            "analyst can read"
        } else {
            "hidden from analyst (app_* is opt-in)"
        };
        println!(
            "    {:<14} schema {:<14} role {:<20} {vis}",
            w.writer.to_string(),
            w.schema,
            w.role
        );
    }

    println!("\n  connect as the read-only analyst");
    println!("    psql '{}'", seeded.analyst_dsn);

    println!("\n  try it");
    println!("    -- allowed: ETL data is analyst-readable by default");
    println!("    SELECT location, sum(net_sales) FROM raw_toast.sales GROUP BY 1;");
    if expose_app {
        println!("    -- allowed: --expose-app opted this schema in");
        println!("    SELECT * FROM app_bookings.orders;");
    } else {
        println!("    -- DENIED (permission denied for schema app_bookings):");
        println!("    SELECT * FROM app_bookings.orders;");
        println!("    -- re-run with --expose-app to opt this schema in");
    }
    println!("    -- DENIED even when visible: the analyst is read-only");
    println!("    UPDATE app_bookings.orders SET status = 'closed';");

    println!("\n  point the Oxy IDE query interface at it — add to config.yml:\n");
    for line in seeded.config_yml_block().lines() {
        println!("    {line}");
    }

    println!("\n  writer DSNs (what a function or pipeline would get)");
    for w in &seeded.writers {
        println!("    {:<14} {}", w.writer.to_string(), w.dsn);
    }
    println!();

    Ok(())
}
