/**
 * Centralized teardown for everything tied to the signed-in user.
 *
 * Called by `AuthContext.logout()` and the axios 401 interceptor so that a
 * different user signing in on the same browser does not inherit the prior
 * user's IDE branch selections, open database tabs, or stale sync state.
 *
 * The theme preference is intentionally kept — it's a device-level setting,
 * not a per-user one, and clearing it makes the page flash on sign-in.
 */

const PERSISTED_AUTH_SCOPED_KEYS = [
  "auth_token",
  "user",
  "ide-branch-storage",
  "database-client-storage",
  "database-operation-storage"
] as const;

export function clearAuthScopedStorage(): void {
  for (const key of PERSISTED_AUTH_SCOPED_KEYS) {
    try {
      localStorage.removeItem(key);
    } catch {
      // localStorage may be disabled (private browsing, quota); failing
      // silently is acceptable — the next session will simply rehydrate
      // whatever survived.
    }
  }
}
