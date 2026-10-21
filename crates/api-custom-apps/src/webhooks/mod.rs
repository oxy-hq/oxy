//! Public webhook receivers for custom-app functions. Mounted on the public seam
//! (outside the auth stack): the sender is proven by an HMAC the platform
//! verifies, not by a session.

pub mod app_function;
pub mod app_function_env;
