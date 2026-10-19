import { isUuid } from "@/libs/utils/uuid";
import type { CustomApp } from "@/types/apps";

/**
 * How the popped-out dossier decides which app it is showing — pure, so the two
 * refusals below are pinned by `dossierWindowApp.test.ts`.
 *
 * `?app=` is text from the address bar. It names a resource in a request URL, so it
 * must be a UUID before it is used at all: anything else is dropped and the window
 * falls back to the slug walk, exactly as a link without the param does.
 */
export function dossierAppId(raw: string | null): string | undefined {
  return isUuid(raw) ? raw.trim().toLowerCase() : undefined;
}

/**
 * The loaded app must be the one the address names. A `?app=` pointing at a
 * different app than `/admin/apps/<org>/<slug>/panel` says is a link that lies, and
 * the window shows an error for it rather than either the id's app or the slug's.
 */
export function dossierAppMatchesRoute(
  app: Pick<CustomApp, "org_slug" | "slug">,
  orgSlug: string | undefined,
  appSlug: string | undefined
): boolean {
  return app.org_slug === orgSlug && app.slug === appSlug;
}

export const DOSSIER_ROUTE_MISMATCH =
  "This link's app doesn't match its address. Open the app from the console again.";
