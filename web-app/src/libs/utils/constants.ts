/**
 * The well-known nil-UUID workspace ID used in local mode. Mirrors
 * `LOCAL_WORKSPACE_ID` in `crates/app/src/server/serve_mode.rs`.
 *
 * In local mode the backend mounts a single implicit workspace under this ID,
 * so the frontend can address per-workspace routes (`/api/{workspace_id}/…`)
 * without ever presenting a workspace picker.
 */
export const LOCAL_WORKSPACE_ID = "00000000-0000-0000-0000-000000000000";

/**
 * The well-known nil-UUID organization ID used in local mode. Mirrors the
 * workspace convention above: when there is no current org (local mode, or
 * cloud mode before the org dispatcher runs), endpoints that take an
 * `org_id` query parameter receive this stand-in instead of throwing on the
 * frontend. The backend will reject it with 403 if no matching org exists,
 * which is the correct user-facing signal.
 */
export const LOCAL_ORG_ID = "00000000-0000-0000-0000-000000000000";
