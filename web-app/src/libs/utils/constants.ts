/**
 * The well-known nil-UUID workspace ID used in local mode. Mirrors
 * `LOCAL_WORKSPACE_ID` in `crates/app/src/server/serve_mode.rs`.
 *
 * In local mode the backend mounts a single implicit workspace under this ID,
 * so the frontend can address per-workspace routes (`/api/{workspace_id}/…`)
 * without ever presenting a workspace picker.
 */
export const LOCAL_WORKSPACE_ID = "00000000-0000-0000-0000-000000000000";
