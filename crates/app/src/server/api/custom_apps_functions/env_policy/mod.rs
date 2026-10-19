//! **What a function may do in the environment it runs in** — one decision per
//! host op (`internal-docs/2026-09-10-custom-app-environments-design.md` §4.1).
//!
//! [`EnvPolicy::decide`] reads one row per [`HostOp`] from the host-op
//! registry (`registry`), and every sub-op is its own row. So a host op added
//! later does not compile until its row states what it does outside
//! production — the posture Tay's hold layer had at runtime (an unclassified
//! op was held), moved to compile time. The broker enforces the row's
//! capability, and the decision of every op that holds by not running,
//! before the op's implementation runs (`runtime::host_call`).
//!
//! **Production is `Allow` for everything**, and a production host behaves
//! exactly as it did before this module existed: every host op asks, and the
//! answer never changes what it does. The one exception is a production run
//! that reads a branch (previews S9, I9): it is a preview, and decides as
//! staging does ([`EnvPolicy::decide_on_branch`]). A staging pin in scope
//! where its host was built makes every op decide so; `ctx.airway.run` also
//! looks up whether the context it would run from is at a `staging` revision.
//!
//! **A sandbox decides as staging does.** A `dev-<handle>` environment
//! (`internal-docs/custom-app-sandboxes.md`) has no table of its own: every
//! "staging" below reads "any non-production environment". What differs per
//! environment is where an isolated write lands — each has its own storage
//! silo, secret path, Airhouse sibling and, inside the org's OLTP staging
//! branch, its own schema (`oltp_home`) — while the branch database itself
//! and the `nonProduction.destinations` map are one per org and per build,
//! shared by staging and every sandbox.
//!
//! **Staging holds every write that has no isolated home.** Reads of
//! production data are allowed (§4.2: staging reads the same warehouse,
//! Airhouse and OLTP data, no copy step). A write with no isolated home is
//! [`Decision::Hold`]: it is not performed, and it is listed in the
//! invocation's `app.staging.held` audit row — the held-write log, which reads
//! as a dry run of what production would have done. `ctx.airway.run` is
//! [`Decision::Refuse`]: a run has no environment, so there is no copy it
//! could ever write instead.
//!
//! How an op holds is the host's mechanism, not a second policy
//! (`host/env_guard.rs`): a pure write is not sent; `ctx.fetch` still sends a
//! read method and answers a held mutating one 409, like `oxyc proxy`; `ctx.oltp`
//! runs in a `READ ONLY` transaction on production, so Postgres itself refuses
//! the write; a held `tx.commit` rolls back.
//!
//! **Phases 4 and 5 flip rows, not callers.** As a store gains an isolated
//! staging copy — the OLTP staging branch, a storage silo, a mapped
//! destination, a sibling Airhouse schema, invoker-only email — its rows move
//! from `Hold` to `Isolate(Target::…)`, and the host op that asked gets the
//! target to write to instead.
//!
//! **An isolated home can depend on what the call names.** A warehouse write
//! is isolated only to a database the build's `nonProduction.destinations`
//! maps ([`EnvPolicy::decide_on_database`]); an unmapped one stays `Hold`. An
//! Airhouse write is isolated to the app schema's sibling
//! ([`EnvPolicy::decide_in_schema`]); a schema that names no sibling stays
//! `Hold`. A statement on a `ctx.tx` handle is decided by where its `begin`
//! was isolated to ([`EnvPolicy::decide_on_handle`]). Each of these only ever
//! turns `Isolate` into `Hold`, never into `Allow`.
//!
//! **One call goes the other way, into a home that already exists.** A
//! mutating `ctx.fetch` is held by its op, except a `PUT` to an upload URL
//! this invocation's own `ctx.storage.getUploadUrl` minted into the
//! environment's silo: that is `Isolate(Target::StorageSilo)`
//! ([`EnvPolicy::decide_on_fetch`], `upload`) — the object `ctx.storage.put`
//! would have written, so a function that uploads through a presigned URL
//! can be exercised outside production. It never becomes `Allow`.
//!
//! **Phase 5a** gives three homes (§4.2): `ctx.storage` works in the
//! environment's own silo ([`Target::StorageSilo`]; its reads fall back to
//! production's same key, read-only), `ctx.email.send` delivers to the invoking
//! user only ([`Target::InvokerEmail`]), and `ctx.secrets.set` writes the
//! environment's secret path ([`Target::EnvSecrets`]). `ctx.env` is not a host
//! op — it is resolved before the isolate starts — so its overlay is read here
//! as data: the manifest's `shared` keys ([`EnvPolicy::with_shared_env`]) and
//! the keys this run read through that fallback, which `ctx.secrets.set`
//! refuses ([`EnvPolicy::read_through_fallback`]).
//!
//! **The OLTP rows depend on the org (P4b).** Whether an org has an OLTP
//! staging branch is decided once, at admission ([`OltpHome`], carried on the
//! policy). With one, `ctx.oltp` and `ctx.oltp.tx` are
//! `Isolate(Target::OltpBranch)`: they read and write the branch — a copy —
//! except a call that reaches outside that database (refused) and SQL decided
//! only when it runs (held) (`oltp_branch_sql`). Without one they hold as
//! above. A sandbox's run on the branch too, in the sandbox's own schema,
//! where a statement naming another schema is refused as well
//! (`oltp_sandbox_sql`); while that schema is not ready they are refused.

pub mod destination_sql;
#[cfg(test)]
mod differential;
mod held;
pub mod homes;
mod oltp_branch_sql;
mod oltp_home;
mod oltp_sandbox_sql;
mod oltp_sql;
#[cfg(test)]
mod tests;
mod upload;

pub use held::{
    HELD_LABEL, REFUSED_LABEL, branch_held_message, branch_refused_message, held_email_result,
    held_fetch_response, held_message, held_notify_result, held_statement_message, is_read_method,
    is_read_only_violation, refused_message,
};
pub use oltp_branch_sql::{
    NotSent, admit_branch_statement, branch_held_statement_message, branch_statement_message,
};
pub use oltp_home::{BRANCH_REFUSED_FIX, NO_BRANCH_NOTE, OltpHome, SandboxHome, SandboxUnready};
pub use oltp_sandbox_sql::{
    SANDBOX_REFUSED_FIX, SandboxFence, admit_sandbox_statement, sandbox_statement_message,
    schema_named_in,
};
pub use oltp_sql::{HeldStatement, admit_oltp_statement};
pub use upload::MintedUpload;

use std::collections::{BTreeMap, BTreeSet};

use oxy_app_core::custom_app_environment::AppEnvironment;
use uuid::Uuid;

/// The registry — one row per op, read by the policy below and by the broker
/// (`runtime::host_call`), which enforces each row's capability and, for an
/// op that holds by not running, its environment decision.
mod op_kinds;
mod registry;
pub use op_kinds::{Capability, Family, HoldBy, HostOpSpec, Staged, Verb};
pub use registry::{AIRWAY_FIX, HostOp};

/// The identifiers a held call is logged under — never values, like every
/// write audit row: `(plane, namespace, verb, table)`.
pub type HeldTarget<'a> = (&'static str, &'a str, &'a str, &'a str);

/// Where an isolated non-production op lands instead of production. A
/// variant exists only once its home does. Phase 4 adds the OLTP staging
/// branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The database the build's `nonProduction.destinations` maps the named
    /// one to ([`EnvPolicy::mapped_destination`]). The mapped name passes the
    /// same destination gate production's does before anything connects.
    MappedDestination,
    /// The app schema's non-production sibling in the workspace's Airhouse,
    /// `app_<writer>__<env>` ([`EnvPolicy::sibling_schema`]). The statement is
    /// checked against the app's own schema, moved, and checked again.
    SiblingSchema,
    /// The environment's sibling asset silo, `customer-app-storage/<id>~<env>/`
    /// (`custom_apps_storage::Silo`). Writes, `delete` and `copy`'s
    /// destination land there; `get`, `head`, `getDownloadUrl` and `copy`'s
    /// source read it first and then production's same relative key,
    /// read-only; `list` lists it alone.
    StorageSilo,
    /// Delivered to the invoking user only, subject prefixed `[<env>]`; the
    /// call's `to` / `cc` / `bcc` are dropped and reported back. A system run
    /// has no invoking user, so its send is refused — never redirected to the
    /// app owner, who did not ask to test anything.
    InvokerEmail,
    /// `ctx.notify` told to the invoking user only, title prefixed `[<env>]`,
    /// whoever the call named; a system run is refused, as for email.
    InvokerNotify,
    /// The environment's secret path, `apps/<id>/<env>/<KEY>`.
    EnvSecrets,
    /// The org's OLTP staging branch (P4a): a copy of production's database,
    /// reached with the branch's own writer credential
    /// (`oxy_oltp::resolver::resolve_branch_writer_connection_for_org`), whose
    /// resolver refuses a row that names production. Staging runs in the
    /// app's schema there; a sandbox in its own ([`OltpHome::SandboxSchema`]).
    OltpBranch,
}

/// What the host does with one op in one environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Performed as asked. Every op in production; reads everywhere.
    Allow,
    /// A write with no isolated home yet: not performed, recorded as held.
    Hold,
    /// A write with an isolated home: performed against `Target`, never
    /// production.
    Isolate(Target),
    /// Never performed outside production; `fix` says what to do instead.
    Refuse { fix: &'static str },
}

/// The one environment decision a function run carries, handed to
/// `ProjectFunctionHost` at construction. No host op re-derives the
/// environment; each asks [`EnvPolicy::decide`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvPolicy {
    environment: AppEnvironment,
    /// The compiled revision this run reads when it is not the promoted one:
    /// a staging build's pin (`custom_apps_staging_pin`), or a pin already in
    /// scope where the run's host was built — a production-admitted run
    /// started inside a workspace preview (previews S9, I9).
    semantic_pin: Option<Uuid>,
    /// The running build's `nonProduction.destinations`: production database
    /// → the database a non-production write to it lands in. Empty in
    /// production, which writes where it is told.
    destinations: BTreeMap<String, String>,
    /// Keys the running build's manifest marks `"shared": true`: the only
    /// ones whose `ctx.env` value may fall back to production's outside
    /// production (§4.2). Empty in production, which has nothing to fall back
    /// from.
    shared_env: BTreeSet<String>,
    /// The shared keys this run's `ctx.env` actually took from production,
    /// because the environment holds no value of its own. `ctx.secrets.set`
    /// refuses them: rotating a credential read from production would fork
    /// production's grant.
    read_through_fallback: BTreeSet<String>,
    /// Where `ctx.oltp` lands, decided once at admission
    /// (`environment_gate::with_oltp_home`): production's own database until
    /// a staging admission finds the org's branch.
    oltp: OltpHome,
}

/// `Refuse` fix for `ctx.airway.run` in a production-admitted run that reads a
/// branch (previews S9, I9).
const BRANCH_AIRWAY_FIX: &str = "an Airway run starts production ELT — production's lease, \
     cursor and destination — whatever revision the invocation reads";

/// `Refuse` fix for an op the policy isolates that reached a path with no
/// isolated home for it — a host bug, refused rather than run on production.
pub const MISROUTED_FIX: &str = "this op has an isolated home outside production, but the call \
     reached a path that does not route to it; nothing was written. Report this as a platform bug";

impl EnvPolicy {
    /// Production: every op allowed. What every run had before environments.
    pub fn production() -> Self {
        Self::for_environment(AppEnvironment::Production)
    }

    /// The policy of `environment`. Built by `environment_gate::admit`, which
    /// decides whether a run may happen there at all; public so a test can
    /// state what an environment's policy is.
    pub fn for_environment(environment: AppEnvironment) -> Self {
        Self {
            environment,
            semantic_pin: None,
            destinations: BTreeMap::new(),
            shared_env: BTreeSet::new(),
            read_through_fallback: BTreeSet::new(),
            oltp: OltpHome::Production,
        }
    }

    /// The same policy reading the build's pin `pin`. Production never takes
    /// one: a promoted build always reads the promoted revision.
    pub fn with_semantic_pin(mut self, pin: Option<Uuid>) -> Self {
        if !self.is_production() {
            self.semantic_pin = pin;
        }
        self
    }

    /// The same policy, keeping a pin already in scope where the run's host is
    /// built (`custom_apps_staging_pin::current_staging_pin`) — host calls run
    /// on tasks of their own, which do not inherit it. A production run that
    /// carries one reads a branch: see [`Self::decide_on_branch`].
    pub fn within_scope_pin(mut self, pin: Option<Uuid>) -> Self {
        self.semantic_pin = self.semantic_pin.or(pin);
        self
    }

    pub fn environment(&self) -> &AppEnvironment {
        &self.environment
    }

    pub fn is_production(&self) -> bool {
        self.environment == AppEnvironment::Production
    }

    pub fn semantic_pin(&self) -> Option<Uuid> {
        self.semantic_pin
    }

    /// Why a production-admitted run reads a branch, when a pin in scope says
    /// it does (previews S9, I9) — the reason its held and refused calls give.
    /// `None` for every other run, staging included.
    pub fn branch_reason(&self) -> Option<String> {
        match (&self.environment, self.semantic_pin) {
            (AppEnvironment::Production, Some(pin)) => {
                Some(format!("it is pinned to staging revision {pin}"))
            }
            _ => None,
        }
    }

    /// May `op` run here, and how? One exhaustive match per environment.
    /// Production allows everything, byte-identical to before environments —
    /// unless a pin in scope says the run reads a branch, when it is a preview
    /// and decides as [`Self::decide_on_branch`].
    pub fn decide(&self, op: HostOp) -> Decision {
        match &self.environment {
            AppEnvironment::Production if self.semantic_pin.is_some() => production_on_branch(op),
            AppEnvironment::Production => Decision::Allow,
            AppEnvironment::Staging | AppEnvironment::Dev { .. } => non_production(op, &self.oltp),
        }
    }

    /// [`Self::decide`] for a run the host has found reading a branch — a pin
    /// in scope, or a project context built at a `staging` revision
    /// (`staged_invocation::staged_reason`). Outside production that is what
    /// the environment already decides; a production run reading a branch is
    /// a preview, and decides as staging does.
    pub fn decide_on_branch(&self, op: HostOp) -> Decision {
        match &self.environment {
            AppEnvironment::Production => production_on_branch(op),
            _ => self.decide(op),
        }
    }
}

/// A production-admitted run reading a branch (previews S9, I9) is a preview:
/// it decides as staging does — its writes held, not live — and
/// `ctx.airway.run` is refused with the reason that applies to it. No caller
/// runs a function on a branch today; one that does later is held, not live.
///
/// It has no environment of its own, so none of staging's isolated homes: a
/// silo, a secret path or a sibling resolved for a production policy would be
/// production's. So an op staging isolates is held here, except the storage
/// reads, which read production's silo as they did before it had a sibling.
/// Its `ctx.oltp` is production's, with no OLTP branch: held as above.
fn production_on_branch(op: HostOp) -> Decision {
    use HostOp::*;
    match op {
        AirwayRun => Decision::Refuse {
            fix: BRANCH_AIRWAY_FIX,
        },
        StorageGetDownloadUrl | StorageGet | StorageHead | StorageList => Decision::Allow,
        _ => match non_production(op, &OltpHome::Production) {
            Decision::Isolate(_) => Decision::Hold,
            decided => decided,
        },
    }
}

/// Staging and every sandbox — one table for every non-production
/// environment, read from the registry (`registry::HostOp::spec`):
/// production data read, a write with an isolated home performed there (the
/// app's OLTP store on the org's staging branch when it has one), every
/// other write held, Airway refused. The table names *which kind* of home;
/// *whose* is the environment's (`homes`: its own silo, secret path and
/// Airhouse sibling). A new [`HostOp`] fails to compile until its row states
/// what it does here.
fn non_production(op: HostOp, oltp: &OltpHome) -> Decision {
    match op.spec().staging {
        Staged::Decided(decided) => decided,
        Staged::ByOltpHome => oltp.decision(),
    }
}

impl EnvPolicy {
    /// What a held `op` throws here: the environment's hold, or a preview's
    /// when a production run reads a branch.
    pub fn held_error(&self, op: HostOp) -> String {
        match self.branch_reason() {
            Some(reason) => branch_held_message(op, &reason),
            None => held_message(op, &self.environment),
        }
    }

    /// What a refused `op` throws here, with `fix`: the environment's
    /// refusal, or a preview's.
    pub fn refusal(&self, op: HostOp, fix: &str) -> String {
        match self.branch_reason() {
            Some(reason) => branch_refused_message(op, &reason, fix),
            None => refused_message(op, &self.environment, fix),
        }
    }
}
