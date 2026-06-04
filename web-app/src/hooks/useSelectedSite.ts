import { useCallback } from "react";
import { useSearchParams } from "react-router-dom";

/**
 * Global site selection used by every site-aware Edge tab.
 *
 * URL is the source of truth (`?site=<uuid>`) — pages can render
 * shareable links without a separate store sync, and "site cleared"
 * is naturally represented by the absence of the param. We use
 * `replace: true` on writes so flipping sites doesn't pollute
 * browser back-stack history with a new entry per click.
 *
 * `siteId === undefined` means "All sites" — the default. Callers
 * use that to skip site-scoped filters.
 *
 * Lives in `hooks/` rather than `pages/ide/edge/hooks/` so future
 * non-Edge surfaces (a fleet widget on the Home page, say) can
 * read the same selection without crossing import layers.
 */
export function useSelectedSite() {
  const [searchParams, setSearchParams] = useSearchParams();
  const siteId = searchParams.get("site") ?? undefined;

  const setSiteId = useCallback(
    (next: string | undefined) => {
      const sp = new URLSearchParams(searchParams);
      if (next === undefined) sp.delete("site");
      else sp.set("site", next);
      setSearchParams(sp, { replace: true });
    },
    [searchParams, setSearchParams]
  );

  return { siteId, setSiteId };
}
