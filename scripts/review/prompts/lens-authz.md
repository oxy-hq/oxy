Focus: authorization decisions (who may do what)

Read first: `crates/authz/CLAUDE.md`, the "Authorization" section of the root CLAUDE.md,
and `internal-docs/roles-and-authorization.md`.

Check:
- Access decided by hand in a handler or service (`matches!(role, Owner | Admin)`,
  reading `OXY_OWNER` or the `app_admins` table, comparing role strings) instead of a
  `role_guards::*` extractor or `server::authz::enforce_guard(..)`.
- Platform standing (`is_owner` / `is_app_admin`) read anywhere but
  `server::authz::globals`.
- A new `Action` without its `Ring`, or without a differential test case. A decision that
  is not `existing_allow && allows(..)`, or passes a constant instead of the real shipped
  check as `existing_allow`.
- `oxy-authz` gaining a dependency beyond `uuid` and `tracing`.
- An org- or workspace-scoped read whose scoping key comes from the request (path, query,
  body) without being checked against the caller's membership: one missing filter that
  exposes another tenant's rows.
- A route reachable by a caller that should not reach it: staff without an assume-role
  session, an App Operator outside its scope, a frontline worker, a sandbox agent token,
  a personal access token above its grant.
- An out-of-scope or unauthorized answer that reveals existence where the house rule is
  to answer "not found".
