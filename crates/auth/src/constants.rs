//! Auth-related string constants.
//!
//! Header names and JWT secret keys lifted out of `oxy::config::constants` so
//! that `oxy-auth` no longer needs to import from `oxy`. The `oxy` crate
//! re-exports these from `oxy::config::constants` for source compatibility.

/// HTTP header used to carry the API key.
pub const DEFAULT_API_KEY_HEADER: &str = "X-API-Key";

/// HTTP header carrying the bearer JWT.
pub const AUTHENTICATION_HEADER_KEY: &str = "authorization";

/// HMAC secret used for signing/verifying built-in JWTs.
pub const AUTHENTICATION_SECRET_KEY: &str = "authentication_secret";
