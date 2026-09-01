import { useCallback, useState } from "react";
import { toast } from "sonner";
import { useRole } from "@/hooks/useRole";
import {
  fetchQuickBooksAuthorizeUrl,
  QB_CONNECT_MESSAGE,
  type QuickBooksAuthorizeBody
} from "@/services/api/quickbooks";

/** Resolved when the user finishes (or abandons) the Intuit consent flow. */
export interface ConnectResult {
  /** QuickBooks realm/company id captured from the callback (popup mode). */
  realmId: string;
}

export interface ConnectArgs {
  clientId: string;
  /** Plaintext secret; omit to reuse the stored one (reconnect). */
  clientSecret?: string;
  clientSecretVar: string;
  refreshTokenVar: string;
  /** Where the browser should return in redirect (mobile) mode. */
  returnPath: string;
  /** Persist transient form state before a full-page redirect. */
  stash?: () => void;
}

const POPUP_TIMEOUT_MS = 3 * 60_000;

/** Popups are unreliable on mobile (blockers + flaky cross-window
 *  postMessage), so prefer the full-page redirect there. */
function prefersRedirect(): boolean {
  return window.matchMedia?.("(pointer: coarse)")?.matches ?? false;
}

/**
 * Drives the QuickBooks "Connect" OAuth flow.
 *
 * - Desktop: opens a popup (synchronously, to keep the user-activation
 *   token), navigates it to the Intuit consent URL, and resolves on a
 *   same-origin `postMessage` from the success page (with a timeout / close
 *   poll). Returns the captured `realmId`.
 * - Mobile / popup blocked: stashes the caller's form state and does a
 *   full-page redirect; the promise does not resolve (the page navigates).
 * - If neither can start, shows an explicit error toast and rejects.
 */
export function useQuickBooksConnect(projectId: string) {
  // Deny only a role we KNOW is not admin. `workspace` is undefined until
  // workspace details load, and refusing on undefined would break the connect
  // button during that window — and anywhere the role never populates — rather
  // than only for a Member. The backend guard is the actual authority; this
  // exists so the popup does not flash open before the 403 arrives.
  const { workspace: wsRole, is } = useRole();
  const isWorkspaceAdmin = wsRole === undefined || is.workspaceAdmin;
  const [connecting, setConnecting] = useState(false);

  const connect = useCallback(
    async (args: ConnectArgs): Promise<ConnectResult | null> => {
      setConnecting(true);
      try {
        const useRedirect = prefersRedirect();

        // The authorize route is WorkspaceAdmin-only (it writes workspace
        // secrets under caller-named keys). Checked here, BEFORE window.open,
        // because that call has to stay synchronous to keep the click's user
        // activation — so failing later would leave a blank popup on screen
        // flashing open and shut before the toast appears.
        if (!isWorkspaceAdmin) {
          toast.error("Only a workspace admin can connect QuickBooks");
          return null;
        }

        // Try popup first on desktop. window.open must run synchronously
        // (before any await) so the click's user-activation isn't lost.
        const popup = useRedirect
          ? null
          : window.open("about:blank", "qb_connect", "width=620,height=760");

        if (!useRedirect && popup) {
          let url: string;
          try {
            url = await fetchQuickBooksAuthorizeUrl(projectId, body(args, "popup"));
          } catch (e) {
            popup.close();
            throw e;
          }
          popup.location.replace(url);
          return await awaitPopupMessage(popup);
        }

        // Redirect path (mobile, or popup blocked). Preserve form state,
        // then hand the whole tab to Intuit.
        args.stash?.();
        const url = await fetchQuickBooksAuthorizeUrl(projectId, body(args, "redirect"));
        window.location.href = url;
        return null; // navigation in progress
      } catch (err) {
        toast.error("Couldn't start QuickBooks Connect", {
          description:
            err instanceof Error
              ? err.message
              : "Allow popups or open Oxy in a desktop browser, then try again."
        });
        throw err;
      } finally {
        setConnecting(false);
      }
    },
    [projectId, isWorkspaceAdmin]
  );

  return { connect, connecting };
}

function body(args: ConnectArgs, mode: "popup" | "redirect"): QuickBooksAuthorizeBody {
  return {
    client_id: args.clientId.trim(),
    client_secret: args.clientSecret?.trim() || undefined,
    client_secret_var: args.clientSecretVar.trim(),
    refresh_token_var: args.refreshTokenVar.trim(),
    mode,
    // Absolute URL of the originating page. The callback redirects the
    // success page to this *frontend* origin (which can differ from the
    // backend/callback origin in dev), and bounces redirect-mode straight
    // back here.
    return_path: args.returnPath
  };
}

/** Resolve on a trusted same-origin success message, or `null` if the popup
 *  closes or the wait times out. */
function awaitPopupMessage(popup: Window): Promise<ConnectResult | null> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (v: ConnectResult | null) => {
      if (settled) return;
      settled = true;
      window.removeEventListener("message", onMessage);
      clearTimeout(timer);
      clearInterval(poll);
      try {
        popup.close();
      } catch {
        // ignore — popup may already be gone
      }
      resolve(v);
    };
    const onMessage = (e: MessageEvent) => {
      if (e.origin !== window.location.origin) return;
      if (e.data?.type !== QB_CONNECT_MESSAGE) return;
      finish({ realmId: typeof e.data.realmId === "string" ? e.data.realmId : "" });
    };
    const timer = setTimeout(() => finish(null), POPUP_TIMEOUT_MS);
    const poll = setInterval(() => {
      if (popup.closed) finish(null);
    }, 1000);
    window.addEventListener("message", onMessage);
  });
}
