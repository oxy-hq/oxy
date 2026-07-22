//! The **only** reader of Oxy's two platform-standing sources.
//!
//! `OXY_OWNER` (an env allow-list) and the `app_admins` table answer one question:
//! is this person Oxy staff, and how senior? That is authorization input, and it used to
//! be read directly from ~20 call sites across handlers, middlewares, the partner tier
//! and the assume path — each re-deciding what "staff" means and what it grants. Two of
//! them combined the flags differently for no stated reason.
//!
//! So the primitives are read here and nowhere else, and callers take one of two doors:
//!
//! * A **decision** — `authz::allows(&facts, Action::Platform*, &Resource::platform())`.
//!   The ring says what staff may do; the call site doesn't restate it.
//! * A **flag to display** — [`platform_standing`], for payloads like `/me` that report
//!   `is_owner` / `is_app_admin` and decide nothing.
//!
//! Keeping both behind this module is what stops the third pattern — a handler
//! hand-rolling `is_oxy_owner() || is_app_admin()` and quietly inventing a policy.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use entity::app_admins;
use entity::prelude::AppAdmins;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};

/// What Oxy's platform sources say about a person. Not a decision — feed it to a ring,
/// or report it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlatformStanding {
    /// In the `OXY_OWNER` env allow-list.
    pub is_global_owner: bool,
    /// In the `app_admins` table.
    pub is_global_admin: bool,
}

impl PlatformStanding {
    /// Either flag — "is this Oxy staff at all". The `oxy_owner_or_app_admin` shape.
    pub fn is_staff(self) -> bool {
        self.is_global_owner || self.is_global_admin
    }
}

/// The owner allow-list alone — an env read with no DB, so sync callers that need only
/// this half don't have to become async to go through the front door.
pub fn is_global_owner(email: &str) -> bool {
    crate::server::api::middlewares::oxy_owner_guard::is_oxy_owner(email)
}

/// TTL for the `app_admins` membership cache. Matches the 60s the check used before it
/// moved here from `customer_apps_auth`.
const ADMIN_CACHE_TTL: Duration = Duration::from_secs(60);

/// Cache of `app_admins` membership, keyed by the normalized email. Self-contained here
/// (rather than reusing `customer_apps_auth`'s cache helper) so authz owns its only
/// `app_admins` read with **no** import back into `customer_apps_*` — that import was a
/// dependency cycle blocking the customer-apps surface from moving.
fn admin_cache() -> &'static RwLock<HashMap<String, (bool, Instant)>> {
    static CACHE: OnceLock<RwLock<HashMap<String, (bool, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cached_admin(email: &str) -> Option<bool> {
    let cache = admin_cache().read().ok()?;
    let (value, at) = cache.get(email)?;
    (at.elapsed() < ADMIN_CACHE_TTL).then_some(*value)
}

fn set_cached_admin(email: String, is_admin: bool) {
    if let Ok(mut cache) = admin_cache().write() {
        // Sweep expired entries so churn of distinct emails can't grow the map unbounded.
        cache.retain(|_, (_, at)| at.elapsed() < ADMIN_CACHE_TTL);
        cache.insert(email, (is_admin, Instant::now()));
    }
}

/// Drop every cached `app_admins` verdict. Callers on the write side — the admin
/// grant/revoke endpoints and the env bootstrap — invalidate after mutating the table so
/// a freshly granted admin isn't masked by a stale cached `false` for up to the TTL.
pub fn invalidate_admin_cache() {
    if let Ok(mut cache) = admin_cache().write() {
        cache.clear();
    }
}

/// Is `email` in the `app_admins` table (a Global Admin)? The `app_admins` read; cached
/// for [`ADMIN_CACHE_TTL`]. `Err` is a lookup failure, distinct from a `false` verdict —
/// [`platform_standing_checked`] is what decides how that unknown collapses.
///
/// Moved here from `customer_apps_auth` so authz owns this read outright; the only other
/// caller is `oxy_app_admin_guard`.
pub async fn is_app_admin_email(db: &DatabaseConnection, email: &str) -> Result<bool, DbErr> {
    let key = email.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Ok(false);
    }
    if let Some(v) = cached_admin(&key) {
        return Ok(v);
    }
    let found = AppAdmins::find()
        .filter(app_admins::Column::Email.eq(key.clone()))
        .one(db)
        .await?
        .is_some();
    set_cached_admin(key, found);
    Ok(found)
}

/// Read the platform sources for `email`, distinguishing **"not staff"** from **"we
/// could not find out"**. `None` is the latter: the `app_admins` lookup errored, so no
/// verdict here is honest.
///
/// That distinction only matters to a decision, which is why it is the door the loader
/// takes. Collapsing an errored lookup to `false` is safe in isolation — it withholds
/// standing rather than inventing it — but under `enforce` it is read as a *fact* that
/// the principal is not staff, and the model then subtracts access their legacy check
/// granted. A wrong 403, from a blip.
pub async fn platform_standing_checked(
    db: &DatabaseConnection,
    email: &str,
) -> Option<PlatformStanding> {
    match is_app_admin_email(db, email).await {
        Ok(is_global_admin) => Some(PlatformStanding {
            is_global_owner: is_global_owner(email),
            is_global_admin,
        }),
        Err(e) => {
            tracing::warn!(
                target: "authz",
                error = %e,
                "app_admins lookup failed — platform standing is unknown, not absent"
            );
            None
        }
    }
}

/// The most standing that can be established with **no database**: the `OXY_OWNER`
/// allow-list, which is an env read.
///
/// This is not a lesser `Default`. `PlatformStanding::default()` says "no standing";
/// this says "no standing *we needed the database for*" — and the difference is a Global
/// Owner keeping their owner-tier UI through a DB outage. Owner status never depended on
/// the DB, so no DB failure should be able to take it away.
pub fn platform_standing_offline(email: &str) -> PlatformStanding {
    PlatformStanding {
        is_global_owner: is_global_owner(email),
        // Genuinely unknown without the `app_admins` table. Withheld, not invented.
        is_global_admin: false,
    }
}

/// Read the platform sources for `email`. The `app_admins` lookup is cached and the
/// owner check is an env read, so this is cheap enough for a per-request payload.
///
/// Fail-closed **only where it has to be**: an unresolvable `app_admins` lookup reports
/// no admin standing rather than granting it, but the owner half falls back to
/// [`platform_standing_offline`] rather than collapsing with it. That is the right
/// behaviour for a **flag to display** (`/me`) and for a call site whose own check reads
/// these same sources. If you are feeding a ring, take [`platform_standing_checked`] and
/// decide for yourself what unknown means.
pub async fn platform_standing(db: &DatabaseConnection, email: &str) -> PlatformStanding {
    for_display(platform_standing_checked(db, email).await, email)
}

/// How an unknown standing collapses for display. Split out from [`platform_standing`]
/// only so it is reachable without a database — this one line IS the bug that motivated
/// the split (`unwrap_or_default()` here silently un-owned a Global Owner), so it should
/// be pinned by a test rather than reviewed by eye.
fn for_display(known: Option<PlatformStanding>, email: &str) -> PlatformStanding {
    known.unwrap_or_else(|| platform_standing_offline(email))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `app_admins` cache moved here with `is_app_admin_email` so that authz no
    /// longer reaches into `customer_apps_auth` for it (that import was a cycle). A
    /// cache that dropped writes would re-query every call — correctness-neutral but
    /// the point of the cache — so pin the round-trip.
    #[test]
    fn admin_cache_round_trips_a_stored_verdict() {
        let email = "admin-cache-probe@oxy.tech";
        assert_eq!(cached_admin(email), None, "a cold cache misses");
        set_cached_admin(email.to_string(), true);
        assert_eq!(
            cached_admin(email),
            Some(true),
            "a warm cache returns the stored verdict, not a re-query"
        );
    }

    /// The regression this exists to prevent: a DB outage must not un-own an owner.
    ///
    /// Both halves used to collapse together onto `PlatformStanding::default()`, which
    /// reported `is_owner: false` at a Global Owner and hid their own UI — over a
    /// failure in a table their standing never depended on.
    #[test]
    #[serial_test::serial(oxy_owner_env)]
    fn offline_standing_keeps_the_owner_flag_it_never_needed_a_database_for() {
        unsafe { std::env::set_var("OXY_OWNER", "owner@oxy.tech") };
        let standing = platform_standing_offline("owner@oxy.tech");
        unsafe { std::env::remove_var("OXY_OWNER") };

        assert!(
            standing.is_global_owner,
            "owner standing is an env read — no database failure is a reason to drop it"
        );
        assert!(
            !standing.is_global_admin,
            "admin standing is genuinely unknown without app_admins; withhold it, don't invent it"
        );
    }

    /// The other half: "no DB" must not become a grant.
    #[test]
    #[serial_test::serial(oxy_owner_env)]
    fn offline_standing_grants_nothing_to_a_non_owner() {
        unsafe { std::env::set_var("OXY_OWNER", "owner@oxy.tech") };
        let standing = platform_standing_offline("someone.else@example.com");
        unsafe { std::env::remove_var("OXY_OWNER") };

        assert_eq!(
            standing,
            PlatformStanding::default(),
            "a caller not on the allow-list has no standing to report offline"
        );
    }

    /// The wiring, not just the helper. This is the assertion that actually fails if
    /// someone writes `unwrap_or_default()` — which is exactly the regression that
    /// shipped, and which a test of `platform_standing_offline` alone would sail past.
    #[test]
    #[serial_test::serial(oxy_owner_env)]
    fn an_unknown_standing_falls_back_to_the_env_not_to_default() {
        unsafe { std::env::set_var("OXY_OWNER", "owner@oxy.tech") };
        let unknown = for_display(None, "owner@oxy.tech");
        let known = for_display(
            Some(PlatformStanding {
                is_global_owner: true,
                is_global_admin: true,
            }),
            "owner@oxy.tech",
        );
        unsafe { std::env::remove_var("OXY_OWNER") };

        assert!(
            unknown.is_global_owner,
            "an app_admins failure must not cost an owner the flag the env already proves"
        );
        assert!(!unknown.is_global_admin, "admin standing stays withheld");
        assert!(
            known.is_global_admin,
            "a known standing must pass through untouched, not be re-derived offline"
        );
    }
}
