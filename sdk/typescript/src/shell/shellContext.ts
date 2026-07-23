// Shell bootstrap data for the wired OxyShell. One bundle-gated request:
//
//   GET /api/projects/:id/shell-context
//
// The server builds every URL in the response (`apps[].url`, `links.*`,
// `logo_url`) because only it knows which host scheme is in play — an org
// subdomain serves apps at `/a/<slug>/` same-origin, while a dedicated
// custom-app subdomain needs absolute product-host URLs. Never derive
// product URLs client-side. See
// `crates/app/src/server/api/custom_apps_shell_context.rs`.

import * as React from "react";
import { getOxyAppLogger } from "../custom-app/logger";
import { useOxyApp } from "../custom-app/react";

export interface ShellContextApp {
  id: string;
  name: string;
  slug: string;
  /** Ready-to-navigate URL (relative on the same host, absolute across hosts). */
  url: string;
  icon_url?: string | null;
  /** Agent ref the app's Ask Oxygen panel binds to (manifest `ask.agent`).
   *  Absent → the shell hides the Ask surface for this app. */
  default_agent?: string | null;
  /** Composer chips for the Ask panel (manifest `ask.suggestedQuestions`). */
  suggested_questions?: string[];
}

export interface ShellContextData {
  workspace: { id: string; name: string };
  org: { id: string; slug: string; name: string };
  /** Workspace logo endpoint URL, or null when no logo is configured. */
  logo_url: string | null;
  /** Published custom apps in this workspace, including the current one. */
  apps: ShellContextApp[];
  /** Product-surface navigation targets, host-aware. `settings` opens the
   *  Unified Settings Dialog via the SPA's `?settings=<section>` deep link
   *  (optional: absent on servers older than the link). */
  links: { home: string; threads: string; settings?: string };
  /** Viewer display identity; null on unauthenticated/local modes. */
  user: { name: string; email: string; picture?: string | null } | null;
}

export interface UseShellContextResult {
  data: ShellContextData | null;
  loading: boolean;
  error: Error | null;
}

/**
 * Fetch the shell bootstrap payload for the current custom app. Must be
 * called inside `<OxyAppProvider>`. Failure is non-fatal by design: the
 * shell degrades to chrome-less rendering (older servers don't have the
 * endpoint), so errors are surfaced on the result, never thrown.
 */
export function useShellContext(): UseShellContextResult {
  const { projectId, fetcher } = useOxyApp();
  const [state, setState] = React.useState<UseShellContextResult>({
    data: null,
    loading: true,
    error: null
  });

  React.useEffect(() => {
    if (!projectId) {
      // No project identity (dev without a server / manifest hint) — nothing
      // to fetch; settle immediately so the shell degrades instead of
      // spinning forever.
      setState({ data: null, loading: false, error: new Error("no projectId resolved") });
      return;
    }
    let cancelled = false;
    fetcher(`/api/projects/${projectId}/shell-context`, { method: "GET" })
      .then(async (resp) => {
        if (!resp.ok) {
          throw new Error(`shell-context failed: HTTP ${resp.status}`);
        }
        const data = (await resp.json()) as ShellContextData;
        if (!cancelled) setState({ data, loading: false, error: null });
      })
      .catch((e: unknown) => {
        const error = e instanceof Error ? e : new Error(String(e));
        getOxyAppLogger().log("warn", "shell-context unavailable, rendering without chrome", {
          error: error.message
        });
        if (!cancelled) setState({ data: null, loading: false, error });
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, fetcher]);

  return state;
}
