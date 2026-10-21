// @vitest-environment jsdom
//
// The SDK half of the `/fn` build handshake: a function call names the build
// its page was served, reacts when the answer says that build is not live, and
// holds automatic reloads for as long as it is in flight.

import * as React from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { _resetCustomAppManifestCacheForTest } from "./manifest";
import { OxyAppProvider, type UseFunctionResult, useFunction } from "./react";
import {
  CLIENT_BUILD_HEADER,
  holdUpdates,
  LIVE_BUILD_HEADER,
  type OxyUpdateAgent,
  withClientBuild
} from "./updates";

const BUILD_A = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const BUILD_B = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const FN_URL = "/customer-apps/acme/store-ops/fn/save-check";

function fakeAgent() {
  const releases: Array<ReturnType<typeof vi.fn>> = [];
  const agent = {
    defer: vi.fn(() => {
      const release = vi.fn();
      releases.push(release);
      return release;
    }),
    apply: vi.fn(() => false),
    check: vi.fn(async () => null),
    status: vi.fn()
  } as unknown as OxyUpdateAgent;
  window.__oxyUpdate = agent;
  return { agent, releases };
}

function recorder(response: () => Response) {
  const calls: Array<{ input: unknown; init?: RequestInit }> = [];
  const base = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ input, init });
    return response();
  }) as unknown as typeof fetch;
  return { base, calls };
}

afterEach(() => {
  delete window.__oxyUpdate;
});

describe("withClientBuild", () => {
  it("names the page's build on a /fn call", async () => {
    const { base, calls } = recorder(() => new Response(null, { status: 204 }));
    await withClientBuild(
      base,
      undefined,
      BUILD_A
    )(FN_URL, {
      method: "POST",
      headers: { "content-type": "application/json" }
    });
    const headers = new Headers(calls[0].init?.headers);
    expect(headers.get(CLIENT_BUILD_HEADER)).toBe(BUILD_A);
    expect(headers.get("content-type")).toBe("application/json");
  });

  it("leaves other paths, cross-origin backends and an unknown build alone", async () => {
    const { base, calls } = recorder(() => new Response(null, { status: 204 }));
    await withClientBuild(base, undefined, BUILD_A)("/api/projects/p1/query");
    await withClientBuild(base, "http://localhost:3000", BUILD_A)(FN_URL);
    await withClientBuild(base, undefined, undefined)(FN_URL);
    for (const c of calls) {
      expect(new Headers(c.init?.headers).has(CLIENT_BUILD_HEADER)).toBe(false);
    }
  });

  it("asks the runtime to check when the function ran in a different build", async () => {
    const { agent } = fakeAgent();
    const live = (id: string) => () =>
      new Response(null, { status: 200, headers: { [LIVE_BUILD_HEADER]: id } });
    await withClientBuild(recorder(live(BUILD_A)).base, undefined, BUILD_A)(FN_URL);
    expect(agent.check).not.toHaveBeenCalled();
    await withClientBuild(recorder(live(BUILD_B)).base, undefined, BUILD_A)(FN_URL);
    expect(agent.check).toHaveBeenCalledTimes(1);
  });

  it("asks the runtime to check on a 409 ClientBuildStale", async () => {
    const { agent } = fakeAgent();
    const stale = () =>
      new Response(JSON.stringify({ error: "ClientBuildStale", live: BUILD_B }), { status: 409 });
    await withClientBuild(recorder(stale).base, undefined, BUILD_A)(FN_URL);
    expect(agent.check).toHaveBeenCalledTimes(1);
  });

  it("is a no-op without the injected runtime", async () => {
    const stale = () => new Response(null, { status: 409 });
    await expect(
      withClientBuild(recorder(stale).base, undefined, BUILD_A)(FN_URL)
    ).resolves.toBeInstanceOf(Response);
    expect(holdUpdates("x")).toBeTypeOf("function");
  });
});

describe("useFunction holds automatic reloads while a call is in flight", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    _resetCustomAppManifestCacheForTest();
    window.__OXY_APP__ = {
      appId: "app-uuid",
      slug: "store-ops",
      orgId: "org-uuid",
      orgSlug: "acme",
      projectId: "proj-uuid",
      branch: "main",
      apiBaseUrl: "",
      buildId: BUILD_A
    };
    globalThis.fetch = (async () =>
      ({
        ok: true,
        status: 200,
        json: async () => ({
          schemaVersion: 2,
          name: "Store Ops",
          slug: "store-ops",
          orgSlug: "acme",
          projectId: "proj-uuid"
        })
      }) as Response) as typeof fetch;
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await React.act(async () => {
      root.unmount();
    });
    container.remove();
    delete window.__OXY_APP__;
  });

  it("defers for the life of the call, sends the build, and releases after", async () => {
    const { agent, releases } = fakeAgent();
    let answer: (r: Response) => void = () => {};
    const sent: RequestInit[] = [];
    const fetcher = ((_input: RequestInfo | URL, init?: RequestInit) => {
      sent.push(init ?? {});
      return new Promise<Response>((resolve) => {
        answer = resolve;
      });
    }) as typeof fetch;

    let fn: UseFunctionResult | undefined;
    function Probe() {
      fn = useFunction("save-check");
      return null;
    }
    await React.act(async () => {
      root.render(
        <OxyAppProvider fetcher={fetcher}>
          <Probe />
        </OxyAppProvider>
      );
    });
    expect(fn).toBeDefined();

    let call: Promise<unknown> | undefined;
    await React.act(async () => {
      call = fn?.invoke({ ok: true }).catch((e: unknown) => e);
    });
    expect(agent.defer).toHaveBeenCalledTimes(1);
    expect(releases[0]).not.toHaveBeenCalled();
    expect(new Headers(sent[0].headers).get(CLIENT_BUILD_HEADER)).toBe(BUILD_A);

    await React.act(async () => {
      answer(
        new Response(JSON.stringify({ error: "ClientBuildStale", live: BUILD_B }), {
          status: 409,
          headers: { "content-type": "application/json" }
        })
      );
      await call;
    });
    expect(releases[0]).toHaveBeenCalledTimes(1);
    expect(agent.check).toHaveBeenCalled();
  });
});
