// @vitest-environment jsdom
//
// The React surface over the injected runtime's update agent: `useAppUpdate`,
// `useBusyWhile`, the shell's "Update ready" pill and crew-mode
// `useKioskIdle`. The runtime itself is tested in the web-app
// (`customAppRuntimeUpdates.test.ts`); here it is a fake `window.__oxyUpdate`.

import * as React from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useKioskIdle } from "../shell/kioskIdle";
import { UpdatePill } from "../shell/UpdatePill";
import { _resetCustomAppManifestCacheForTest } from "./manifest";
import { OxyAppProvider } from "./react";
import { type AppUpdateState, useAppUpdate, useBusyWhile } from "./update-hooks";
import type { OxyUpdateAgent, OxyUpdateStatus } from "./updates";

const BUILD_A = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const BUILD_B = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

let container: HTMLDivElement;
let root: Root;

function installAgent(status: Partial<OxyUpdateStatus> = {}) {
  const state: OxyUpdateStatus = {
    mode: "observe",
    current: BUILD_A,
    available: null,
    sessionExpired: false,
    ...status
  };
  const releases: Array<ReturnType<typeof vi.fn>> = [];
  const agent = {
    defer: vi.fn(() => {
      const release = vi.fn();
      releases.push(release);
      return release;
    }),
    apply: vi.fn(() => true),
    check: vi.fn(async () => null),
    status: vi.fn(() => ({ ...state }))
  } as unknown as OxyUpdateAgent;
  window.__oxyUpdate = agent;
  return {
    agent,
    releases,
    announce(to: string) {
      state.available = to;
      window.dispatchEvent(new CustomEvent("oxy:update-available", { detail: { to } }));
    },
    /** A change the runtime announces only through `oxy:update-status`. */
    change(next: Partial<OxyUpdateStatus>) {
      Object.assign(state, next);
      window.dispatchEvent(new CustomEvent("oxy:update-status"));
    }
  };
}

async function render(node: React.ReactNode) {
  await React.act(async () => {
    root.render(node);
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(async () => {
  await React.act(async () => {
    root.unmount();
  });
  container.remove();
  delete window.__oxyUpdate;
  vi.unstubAllGlobals();
});

describe("useAppUpdate", () => {
  it("reflects an announcement made after mount, and applies through the runtime", async () => {
    const fake = installAgent();
    let seen: AppUpdateState | undefined;
    function Probe() {
      seen = useAppUpdate();
      return null;
    }
    await render(<Probe />);
    expect(seen?.available).toBeNull();
    expect(seen?.mode).toBe("observe");

    await React.act(async () => {
      fake.announce(BUILD_B);
    });
    expect(seen?.available).toBe(BUILD_B);
    expect(seen?.apply()).toBe(true);
    expect(fake.agent.apply).toHaveBeenCalledTimes(1);
  });

  it("follows a rollback, a re-detection and a restored session", async () => {
    const fake = installAgent({ available: BUILD_B, sessionExpired: true });
    let seen: AppUpdateState | undefined;
    function Probe() {
      seen = useAppUpdate();
      return null;
    }
    await render(<Probe />);
    expect(seen?.available).toBe(BUILD_B);
    await React.act(async () => {
      fake.change({ available: null, sessionExpired: false });
    });
    expect(seen?.available).toBeNull();
    expect(seen?.sessionExpired).toBe(false);
    await React.act(async () => {
      fake.change({ available: BUILD_B });
    });
    expect(seen?.available).toBe(BUILD_B);
  });

  it("is inert where oxy did not serve the page", async () => {
    let seen: AppUpdateState | undefined;
    function Probe() {
      seen = useAppUpdate();
      return null;
    }
    await render(<Probe />);
    expect(seen?.available).toBeNull();
    expect(seen?.mode).toBeUndefined();
    expect(seen?.apply()).toBe(false);
  });
});

describe("useBusyWhile", () => {
  it("holds updates exactly while the condition is true", async () => {
    const fake = installAgent();
    function Form({ dirty }: { dirty: boolean }) {
      useBusyWhile(dirty, "unsaved checklist");
      return null;
    }
    await render(<Form dirty={false} />);
    expect(fake.agent.defer).not.toHaveBeenCalled();
    await render(<Form dirty={true} />);
    expect(fake.agent.defer).toHaveBeenCalledWith("unsaved checklist");
    expect(fake.releases[0]).not.toHaveBeenCalled();
    await render(<Form dirty={false} />);
    expect(fake.releases[0]).toHaveBeenCalledTimes(1);
  });
});

describe("UpdatePill", () => {
  function stubStandalone(matches: boolean) {
    vi.stubGlobal(
      "matchMedia",
      vi.fn(() => ({ matches, addEventListener: vi.fn(), removeEventListener: vi.fn() }))
    );
  }

  it("shows in prompt mode once an update is available, and applies on tap", async () => {
    stubStandalone(false);
    const fake = installAgent({ mode: "prompt" });
    await render(<UpdatePill />);
    expect(container.querySelector("[data-testid=topbar-update-ready]")).toBeNull();
    await React.act(async () => {
      fake.announce(BUILD_B);
    });
    const pill = container.querySelector<HTMLButtonElement>("[data-testid=topbar-update-ready]");
    expect(pill).not.toBeNull();
    await React.act(async () => {
      pill?.click();
    });
    expect(fake.agent.apply).toHaveBeenCalledTimes(1);
  });

  it("stays hidden in a browser tab in auto mode, but shows in an installed PWA", async () => {
    stubStandalone(false);
    installAgent({ mode: "auto", available: BUILD_B });
    await render(<UpdatePill />);
    expect(container.querySelector("[data-testid=topbar-update-ready]")).toBeNull();
    await React.act(async () => {
      root.unmount();
    });

    root = createRoot(container);
    stubStandalone(true);
    await render(<UpdatePill />);
    expect(container.querySelector("[data-testid=topbar-update-ready]")).not.toBeNull();
  });
});

describe("useKioskIdle", () => {
  const MANIFEST = {
    schemaVersion: 2,
    name: "Store Ops",
    slug: "store-ops",
    orgSlug: "acme",
    projectId: "proj-uuid"
  };

  beforeEach(() => {
    _resetCustomAppManifestCacheForTest();
    globalThis.fetch = (async () =>
      ({ ok: true, status: 200, json: async () => MANIFEST }) as Response) as typeof fetch;
  });

  function kioskFetcher(device: Record<string, unknown>) {
    const urls: string[] = [];
    const fetcher = (async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      urls.push(url);
      if (url === "/api/frontline/device") {
        return { ok: true, status: 200, json: async () => device } as Response;
      }
      return { ok: true, status: 200 } as Response;
    }) as typeof fetch;
    return { fetcher, urls };
  }

  function Kiosk() {
    useKioskIdle();
    return null;
  }

  it("signs out after the kiosk's idle limit, and input resets the clock", async () => {
    const { fetcher, urls } = kioskFetcher({ bound: true, idleTimeoutSeconds: 1 });
    await render(
      <OxyAppProvider fetcher={fetcher}>
        <Kiosk />
      </OxyAppProvider>
    );
    await React.act(async () => {
      await new Promise((r) => setTimeout(r, 600));
      window.dispatchEvent(new Event("pointerdown"));
      await new Promise((r) => setTimeout(r, 600));
    });
    expect(urls).not.toContain("/api/logout");
    await React.act(async () => {
      await new Promise((r) => setTimeout(r, 700));
    });
    expect(urls).toContain("/api/logout");
  });

  it("does nothing on a browser that is not an enrolled kiosk", async () => {
    const { fetcher, urls } = kioskFetcher({ bound: false });
    await render(
      <OxyAppProvider fetcher={fetcher}>
        <Kiosk />
      </OxyAppProvider>
    );
    await React.act(async () => {
      await new Promise((r) => setTimeout(r, 1200));
    });
    expect(urls).toContain("/api/frontline/device");
    expect(urls).not.toContain("/api/logout");
  });
});
