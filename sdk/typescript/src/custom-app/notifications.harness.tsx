// Shared by the inbox's jsdom tests (not itself a test file): an app page with
// identity, a manifest, and a root to mount into behind `<OxyAppProvider>`.

import * as React from "react";
import { createRoot, type Root } from "react-dom/client";
import { vi } from "vitest";
import type { OxyAppEnvironment } from "./inject";
import { _resetCustomAppManifestCacheForTest } from "./manifest";
import { OxyAppProvider } from "./react";

export const APP_ID = "app-1";

// Tell React this is a test runner, so `act` flushes without a warning.
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/**
 * Identity and manifest as oxy serves them; `appId: null` is `pnpm dev`, which
 * has none. `environment` is the host's (absent: a server older than them).
 */
export function servePage(appId: string | null = APP_ID, environment?: OxyAppEnvironment): void {
  _resetCustomAppManifestCacheForTest();
  window.__OXY_APP__ = {
    environment,
    appId: (appId ?? undefined) as string,
    slug: "bookings",
    orgId: "org-1",
    orgSlug: "acme",
    projectId: "proj-1",
    branch: "main",
    apiBaseUrl: ""
  };
  globalThis.fetch = (async () =>
    new Response(JSON.stringify({ schemaVersion: 2, slug: "bookings", orgSlug: "acme" }), {
      status: 200
    })) as typeof fetch;
}

let hidden = false;
/** Drive `document.hidden` and fire `visibilitychange`, as a tab switch does. */
export function setHidden(value: boolean): void {
  hidden = value;
  document.dispatchEvent(new Event("visibilitychange"));
}
Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });

export interface Page {
  container: HTMLDivElement;
  unmount(): Promise<void>;
}

/** Render `ui` behind the provider, wired to `fetcher`, and let the manifest load. */
export async function mount(ui: React.ReactNode, fetcher: typeof fetch): Promise<Page> {
  hidden = false;
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root: Root = createRoot(container);
  await React.act(async () => {
    root.render(<OxyAppProvider fetcher={fetcher}>{ui}</OxyAppProvider>);
  });
  await settle();
  return {
    container,
    unmount: async () => {
      await React.act(async () => root.unmount());
      container.remove();
    }
  };
}

/** Let `ms` of fake time pass, with every promise and render it causes. */
export async function settle(ms = 0): Promise<void> {
  await React.act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/** Click `el` the way a person does, inside `act`. */
export async function click(el: Element | null): Promise<void> {
  if (!el) throw new Error("nothing to click");
  await React.act(async () => {
    el.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
  await settle();
}
