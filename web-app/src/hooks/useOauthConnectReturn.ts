import { useEffect } from "react";
import { toast } from "sonner";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useSettingsDialog from "@/stores/useSettingsDialog";

/**
 * Closes the loop on a Settings → Connections OAuth redirect.
 *
 * The consent flow leaves the SPA entirely and comes back to `return_path` with
 * `?oxy_connected=ok`. The settings dialog is zustand state, not URL state, so
 * without this the user lands on a page with the dialog shut, no confirmation
 * that anything happened, and a stale query string — indistinguishable from
 * having cancelled.
 *
 * The flag is stripped with `replaceState` so a refresh or a shared link does
 * not re-toast.
 */
export default function useOauthConnectReturn(): void {
  const open = useSettingsDialog((s) => s.open);
  // An OAuth return is a FULL PAGE LOAD, and neither the workspace nor the org
  // store is persisted — so on the first commit after this mounts the role is
  // undefined, and `SettingsDialog` has filtered `workspace.connections` out of
  // the nav (its gate reads `is.workspaceAdmin`, false while undefined).
  //
  // Opening then does not merely fail. `SettingsDialog` falls back to
  // `allItems[0]` and writes that back into the store; with no workspace and no
  // org loaded yet the only surviving group is Preferences, so the selection
  // becomes "preferences.appearance" and STICKS — once details land, that value
  // is legitimately in the nav and the sync effect has nothing to correct.
  //
  // So wait for the role. The effect re-runs when this flips, which is why the
  // early return must sit ABOVE the strip below: the `oxy_connected` flag has to
  // survive the wait, or the reopen has nothing left to act on.
  const roleLoaded = useCurrentWorkspace((s) => s.workspace?.current_user_role) !== undefined;

  useEffect(() => {
    if (!roleLoaded) return;

    const url = new URL(window.location.href);
    if (url.searchParams.get("oxy_connected") !== "ok") return;

    url.searchParams.delete("oxy_connected");
    // QuickBooks' own flow appends this; harmless here but it would otherwise
    // linger in the address bar after a Drive connect.
    url.searchParams.delete("realm_id");
    window.history.replaceState({}, "", url.toString());

    toast.success("Connected. The refresh token is stored in workspace secrets.");
    // Deliberately NOT also gated on `is.workspaceAdmin`: a non-admin holding
    // this flag would then never have it stripped, leaving the query string
    // dirty indefinitely. `Connections` shows its own access notice for a role
    // known not to be admin, and `authorize` sits behind the WorkspaceAdmin
    // extractor regardless — so opening it is coherent, not a leak.
    open("workspace.connections");
  }, [open, roleLoaded]);
}
