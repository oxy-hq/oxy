//! `OxyOwnerGuard` — Oxy-staff-only gate for `/admin/*`.
//!
//! Reads the authenticated user (inserted upstream by `auth_middleware`)
//! and checks the email against the comma-separated `OXY_OWNER` allow-list
//! (case-insensitive, whitespace-trimmed). Returns `403 FORBIDDEN` if
//! `OXY_OWNER` is unset/empty or the caller's email isn't allowed, and
//! `401 UNAUTHORIZED` if no authenticated user is present.
//!
//! Applied as a router layer in `router::global` so all `/admin/*`
//! endpoints are gated by default — handlers don't need to repeat the
//! check, and new admin routes are guarded automatically.

use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use oxy_auth::types::AuthenticatedUser;

pub async fn oxy_owner_guard_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let email = request
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.email.clone())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    require_oxy_owner(&email)?;
    Ok(next.run(request).await)
}

/// Returns `true` when `email` matches the `OXY_OWNER` allow-list.
///
/// Same matching rules as the middleware (case- and whitespace-insensitive,
/// comma-separated). Used by login responses to expose `is_owner` on the
/// user payload so the frontend can route owners to the admin shell. The
/// server-side middleware remains the authoritative gate for `/admin/*`.
pub fn is_oxy_owner(email: &str) -> bool {
    let allow = std::env::var("OXY_OWNER").unwrap_or_default();
    if allow.is_empty() {
        return false;
    }
    let needle = email.trim().to_ascii_lowercase();
    allow
        .split(',')
        .any(|e| e.trim().to_ascii_lowercase() == needle)
}

fn require_oxy_owner(email: &str) -> Result<(), StatusCode> {
    if is_oxy_owner(email) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }

        fn unset(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn forbids_when_env_unset() {
        let _g = EnvGuard::unset("OXY_OWNER");
        assert_eq!(
            require_oxy_owner("anyone@example.com"),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn forbids_non_owner() {
        let _g = EnvGuard::set("OXY_OWNER", "owner@oxy.tech");
        assert_eq!(
            require_oxy_owner("intruder@example.com"),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn allows_owner_case_and_whitespace_insensitive() {
        let _g = EnvGuard::set("OXY_OWNER", " Owner@Oxy.Tech , other@oxy.tech ");
        assert_eq!(require_oxy_owner("owner@oxy.tech"), Ok(()));
        assert_eq!(require_oxy_owner("OTHER@OXY.TECH"), Ok(()));
    }
}
