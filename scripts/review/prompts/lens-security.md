Focus: application security (injection, secrets, authentication, untrusted input)

Read first: the "Authentication" bullet and the "Counterintuitive gotchas" section of the
product context in the root CLAUDE.md.

Check:
- SQL, shell commands, file paths or templates built from request-controlled input
  without parameters or escaping: `format!` into SQL, `Command::new` with user data, path
  joins that allow `..`.
- Secrets, tokens or credentials logged, returned in a response, put in an error message,
  stored in plaintext, or compared without a constant-time comparison.
- Authentication: token or session validation skipped on a path, expiry not enforced, a
  redirect target or `return_to` not validated (open redirect), a cookie missing
  `Secure`, `HttpOnly` or `SameSite` where the neighbouring code sets them.
- Webhook or signature verification that can be bypassed: verified after use, checked on
  re-serialized JSON instead of the raw bytes, missing on one branch.
- User content rendered as HTML (`dangerouslySetInnerHTML`, `innerHTML`) without
  sanitising.
- `unsafe` Rust without a justification that holds.
- A server-side fetch of a URL a user controls, without an allowlist (SSRF).
