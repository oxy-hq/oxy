/**
 * `oxyc apps domains [list] | add | verify | remove`, driven with
 * `globalThis.fetch` stubbed by a route table, as `apps.test.ts` does.
 *
 * Pinned: the app resolves as every `apps` verb does; `verify` and `remove`
 * find the row by NAME and send the row's id; a domain the app does not have
 * is NOT_FOUND before anything is written; a `verify` that is still pending
 * prints the checker's reason and exits non-zero; and the server's
 * `{error, message}` refusals keep their message and map by status.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Context } from "../context/resolve.js";
import { CliError, ExitCode } from "../util/errors.js";
import {
  runAppsDomainsAdd,
  runAppsDomainsList,
  runAppsDomainsRemove,
  runAppsDomainsVerify
} from "./apps-domains.js";

const TARGET = "https://oxy.test";
const APP_ID = "aaaaaaaa-1111-4111-8111-111111111111";
const APP = "acme/store";
const LISTING = "/api/customer-apps?limit=100&offset=0";
const DOMAINS = `/api/customer-apps/${APP_ID}/domains`;
const CNAME = "edge.oxy.test";

function fakeContext(env = "production"): Context {
  return {
    cwd: "/tmp",
    flags: { env, apiKeyEnv: "OXY_API_KEY" },
    target: () => TARGET,
    env: () => ({ target: TARGET, orgSlug: undefined }) as ReturnType<Context["env"]>,
    bearer: async () => "tok",
    maybeBearer: async () => "tok",
    storedBearer: () => "tok",
    async credential() {
      return { token: await this.bearer(), source: "env" };
    },
    serviceAccount: () => undefined,
    apiKey: () => undefined,
    customer: () => undefined,
    repoDir: () => undefined,
    placeholders: () => ({}),
    withEnv: () => fakeContext(env)
  } as Context;
}

interface Reply {
  status: number;
  body?: unknown;
}

/** One handler per `METHOD path`; anything else answers 404. Bodies sent are recorded. */
function stubFetch(routes: Record<string, () => Reply>, calls: string[], sent: unknown[] = []) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL, init?: RequestInit) => {
      const key = `${(init?.method ?? "GET").toUpperCase()} ${String(input).replace(TARGET, "")}`;
      calls.push(key);
      if (typeof init?.body === "string") sent.push(JSON.parse(init.body));
      const handler = routes[key];
      if (!handler) return new Response(JSON.stringify({ error: "no stub" }), { status: 404 });
      const { status, body } = handler();
      if (body === undefined) return new Response(null, { status });
      return new Response(JSON.stringify(body), { status });
    })
  );
}

function capture(): { stdout: () => string; stderr: () => string } {
  const chunks = { out: [] as string[], err: [] as string[] };
  vi.spyOn(process.stdout, "write").mockImplementation((chunk) => {
    chunks.out.push(String(chunk));
    return true;
  });
  vi.spyOn(process.stderr, "write").mockImplementation((chunk) => {
    chunks.err.push(String(chunk));
    return true;
  });
  return { stdout: () => chunks.out.join(""), stderr: () => chunks.err.join("") };
}

async function failure(run: Promise<void>): Promise<CliError> {
  const cause = await run.then(
    () => undefined,
    (e: unknown) => e
  );
  expect(cause).toBeInstanceOf(CliError);
  return cause as CliError;
}

const appRow = {
  id: APP_ID,
  slug: "store",
  name: "store",
  org_id: "org",
  org_slug: "acme",
  project_id: "project",
  branch: "main",
  source_repo: "oxy-hq/customer-apps",
  status: "created",
  url: `${TARGET}/customer-apps/acme/store/`,
  published_at: "2026-09-10T08:00:00+00:00",
  created_at: "2026-09-01T00:00:00+00:00",
  updated_at: "2026-09-01T00:00:00+00:00"
};

function domain(over: Record<string, unknown> = {}) {
  return {
    id: "dom-1",
    domain: "app.acme.com",
    status: "pending",
    url: "https://app.acme.com/",
    txt_name: "_oxy-challenge.app.acme.com",
    txt_value: "oxy-verify=abc123",
    verified_at: null,
    last_checked_at: null,
    last_check_error: null,
    created_at: "2026-10-20T00:00:00+00:00",
    ...over
  };
}

const VERIFIED = domain({
  id: "dom-2",
  domain: "shop.acme.com",
  status: "verified",
  url: "https://shop.acme.com/",
  verified_at: "2026-10-21T09:30:00+00:00"
});

/** The listing, so `<app>` resolves, plus whatever the test adds. */
function routes(extra: Record<string, () => Reply>): Record<string, () => Reply> {
  return {
    [`GET ${LISTING}`]: () => ({ status: 200, body: { items: [appRow], next_offset: null } }),
    ...extra
  };
}

let calls: string[];
beforeEach(() => {
  calls = [];
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("apps domains (list)", () => {
  const listRoutes = routes({
    [`GET ${DOMAINS}`]: () => ({
      status: 200,
      body: {
        domains: [domain({ last_check_error: "no CNAME record found for app.acme.com" }), VERIFIED],
        cname_target: CNAME
      }
    })
  });

  it("lists each domain and the DNS records of the pending ones only", async () => {
    stubFetch(listRoutes, calls);
    const io = capture();

    await runAppsDomainsList(fakeContext(), APP, false);

    const text = io.stdout();
    expect(text).toContain("app.acme.com");
    expect(text).toContain("shop.acme.com");
    expect(text).toContain("https://shop.acme.com/");
    expect(text).toContain(`CNAME  app.acme.com  →  ${CNAME}`);
    expect(text).toContain("TXT    _oxy-challenge.app.acme.com  =  oxy-verify=abc123");
    expect(text).toContain("last check failed: no CNAME record found for app.acme.com");
    expect(text).toContain(`oxyc apps domains verify ${APP} app.acme.com`);
    // The verified domain needs no records.
    expect(text).not.toContain("CNAME  shop.acme.com");
  });

  it("tells the operator to ask Oxy ops when the deployment has no CNAME target", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: () => ({
          status: 200,
          body: { domains: [domain()], cname_target: null }
        })
      }),
      calls
    );
    const io = capture();

    await runAppsDomainsList(fakeContext(), APP, false);

    expect(io.stdout()).toMatch(/CNAME {2}app\.acme\.com {2}→ {2}.*ask Oxy ops/);
  });

  it("--json is the server's response with the app named", async () => {
    stubFetch(listRoutes, calls);
    const io = capture();

    await runAppsDomainsList(fakeContext(), APP, true);

    const parsed = JSON.parse(io.stdout()) as Record<string, unknown>;
    expect(parsed).toMatchObject({ app: APP, app_id: APP_ID, cname_target: CNAME });
    expect(parsed.domains).toHaveLength(2);
  });

  it("says so, and exits 0, when the app has no domains", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: () => ({ status: 200, body: { domains: [], cname_target: CNAME } })
      }),
      calls
    );
    const io = capture();

    await runAppsDomainsList(fakeContext(), APP, false);

    expect(io.stdout()).toContain(`oxyc apps domains add ${APP} <domain>`);
  });

  it("accepts an app UUID", async () => {
    stubFetch(listRoutes, calls);
    capture();

    await runAppsDomainsList(fakeContext(), APP_ID, false);

    expect(calls).toContain(`GET ${DOMAINS}`);
  });

  it("refuses an <app> that is neither <org>/<app> nor a UUID, before any request", async () => {
    stubFetch(listRoutes, calls);

    const error = await failure(runAppsDomainsList(fakeContext(), "store", false));

    expect(error.code).toBe(ExitCode.USAGE);
    expect(calls).toEqual([]);
  });

  it("rejects with NOT_FOUND for an app no page holds", async () => {
    stubFetch(listRoutes, calls);

    const error = await failure(runAppsDomainsList(fakeContext(), "acme/nope", false));

    expect(error.code).toBe(ExitCode.NOT_FOUND);
  });

  it("rejects, rather than printing an empty list, when the domains route is not there", async () => {
    stubFetch(routes({}), calls);

    const error = await failure(runAppsDomainsList(fakeContext(), APP, false));

    expect(error.code).toBe(ExitCode.NOT_FOUND);
  });
});

describe("apps domains add", () => {
  const added = domain();

  it("posts the domain and prints both DNS records and the verify command", async () => {
    const sent: unknown[] = [];
    stubFetch(
      routes({
        [`POST ${DOMAINS}`]: () => ({ status: 201, body: added }),
        [`GET ${DOMAINS}`]: () => ({
          status: 200,
          body: { domains: [added], cname_target: CNAME }
        })
      }),
      calls,
      sent
    );
    const io = capture();

    await runAppsDomainsAdd(fakeContext("staging"), APP, " App.Acme.com ", false);

    expect(sent).toEqual([{ domain: "App.Acme.com" }]);
    const text = io.stdout();
    expect(text).toContain(`CNAME  app.acme.com  →  ${CNAME}`);
    expect(text).toContain("TXT    _oxy-challenge.app.acme.com  =  oxy-verify=abc123");
    // The next step repeats a non-default --env.
    expect(text).toContain(`oxyc apps domains verify ${APP} app.acme.com --env staging`);
  });

  it("still reports the added domain when the CNAME target cannot be read", async () => {
    stubFetch(routes({ [`POST ${DOMAINS}`]: () => ({ status: 201, body: added }) }), calls);
    const io = capture();

    await runAppsDomainsAdd(fakeContext(), APP, "app.acme.com", false);

    expect(io.stderr()).toContain("CNAME target could not be read");
    expect(io.stdout()).toContain("ask Oxy ops");
  });

  it("--json carries the new domain and the CNAME target", async () => {
    stubFetch(
      routes({
        [`POST ${DOMAINS}`]: () => ({ status: 201, body: added }),
        [`GET ${DOMAINS}`]: () => ({
          status: 200,
          body: { domains: [added], cname_target: CNAME }
        })
      }),
      calls
    );
    const io = capture();

    await runAppsDomainsAdd(fakeContext(), APP, "app.acme.com", true);

    expect(JSON.parse(io.stdout())).toMatchObject({
      app: APP,
      domain: { id: "dom-1", status: "pending" },
      cname_target: CNAME
    });
  });

  it.each([
    [400, "invalid_domain", "not a valid domain name", ExitCode.REQUEST],
    [409, "domain_exists", "this app already has that domain", ExitCode.REQUEST]
  ])("keeps the server's message on a %i %s", async (status, error, message, exit) => {
    stubFetch(routes({ [`POST ${DOMAINS}`]: () => ({ status, body: { error, message } }) }), calls);

    const failed = await failure(runAppsDomainsAdd(fakeContext(), APP, "app.acme.com", false));

    expect(failed.message).toBe(message);
    expect(failed.serverCode).toBe(error);
    expect(failed.code).toBe(exit);
  });

  it("maps a 403 to AUTH", async () => {
    stubFetch(
      routes({ [`POST ${DOMAINS}`]: () => ({ status: 403, body: { message: "no" } }) }),
      calls
    );

    const failed = await failure(runAppsDomainsAdd(fakeContext(), APP, "app.acme.com", false));

    expect(failed.code).toBe(ExitCode.AUTH);
  });

  it("refuses a URL or an empty name before any request", async () => {
    stubFetch(routes({}), calls);
    for (const bad of ["https://app.acme.com/", "app.acme.com/path", "  ", "two words"]) {
      const failed = await failure(runAppsDomainsAdd(fakeContext(), APP, bad, false));
      expect(failed.code, bad).toBe(ExitCode.USAGE);
    }
    expect(calls).toEqual([]);
  });
});

describe("apps domains verify", () => {
  const pending = domain({ last_checked_at: "2026-10-22T10:00:00+00:00" });
  const list = () => ({ status: 200, body: { domains: [pending, VERIFIED], cname_target: CNAME } });
  const verifyPath = `${DOMAINS}/dom-1/verify`;

  it("finds the row by name (any case) and posts to its verify route", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: list,
        [`POST ${verifyPath}`]: () => ({
          status: 200,
          body: domain({ status: "verified", verified_at: "2026-10-22T10:05:00+00:00" })
        })
      }),
      calls
    );
    const io = capture();

    await runAppsDomainsVerify(fakeContext(), APP, "APP.Acme.com.", false);

    expect(calls).toContain(`POST ${verifyPath}`);
    expect(io.stdout()).toContain("app.acme.com is verified — https://app.acme.com/");
  });

  it("exits FAILURE with the checker's reason while the domain is still pending", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: list,
        [`POST ${verifyPath}`]: () => ({
          status: 200,
          body: { ...pending, last_check_error: "TXT record _oxy-challenge.app.acme.com not found" }
        })
      }),
      calls
    );
    const io = capture();

    const failed = await failure(runAppsDomainsVerify(fakeContext(), APP, "app.acme.com", false));

    expect(failed.code).toBe(ExitCode.FAILURE);
    expect(failed.message).toContain("not verified yet");
    expect(failed.detail).toContain("TXT record _oxy-challenge.app.acme.com not found");
    // The records are printed again so the operator can fix them.
    expect(io.stdout()).toContain("still pending");
    expect(io.stdout()).toContain(`CNAME  app.acme.com  →  ${CNAME}`);
  });

  it("--json prints the state, then still exits non-zero when pending", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: list,
        [`POST ${verifyPath}`]: () => ({ status: 200, body: pending })
      }),
      calls
    );
    const io = capture();

    const failed = await failure(runAppsDomainsVerify(fakeContext(), APP, "app.acme.com", true));

    expect(failed.code).toBe(ExitCode.FAILURE);
    expect(JSON.parse(io.stdout())).toMatchObject({ domain: { status: "pending" } });
  });

  it("exits NOT_FOUND, writing nothing, for a domain the app does not have", async () => {
    stubFetch(routes({ [`GET ${DOMAINS}`]: list }), calls);

    const failed = await failure(runAppsDomainsVerify(fakeContext(), APP, "other.acme.com", false));

    expect(failed.code).toBe(ExitCode.NOT_FOUND);
    expect(failed.hint).toContain("app.acme.com, shop.acme.com");
    expect(calls.some((c) => c.startsWith("POST"))).toBe(false);
  });

  it("keeps the message of a 409 domain_taken", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: list,
        [`POST ${verifyPath}`]: () => ({
          status: 409,
          body: { error: "domain_taken", message: "another app already serves app.acme.com" }
        })
      }),
      calls
    );

    const failed = await failure(runAppsDomainsVerify(fakeContext(), APP, "app.acme.com", false));

    expect(failed.serverCode).toBe("domain_taken");
    expect(failed.message).toBe("another app already serves app.acme.com");
    expect(failed.code).toBe(ExitCode.REQUEST);
  });
});

describe("apps domains remove", () => {
  const list = () => ({
    status: 200,
    body: { domains: [domain(), VERIFIED], cname_target: CNAME }
  });

  it("deletes the row found by name, by its id", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: list,
        [`DELETE ${DOMAINS}/dom-2`]: () => ({ status: 204 })
      }),
      calls
    );
    const io = capture();

    await runAppsDomainsRemove(fakeContext(), APP, "shop.acme.com", false);

    expect(calls).toContain(`DELETE ${DOMAINS}/dom-2`);
    expect(io.stdout()).toContain("Removed shop.acme.com");
    expect(io.stdout()).toContain("no longer serves this app");
  });

  it("--json names what was removed", async () => {
    stubFetch(
      routes({ [`GET ${DOMAINS}`]: list, [`DELETE ${DOMAINS}/dom-1`]: () => ({ status: 204 }) }),
      calls
    );
    const io = capture();

    await runAppsDomainsRemove(fakeContext(), APP, "app.acme.com", true);

    expect(JSON.parse(io.stdout())).toEqual({
      app: APP,
      app_id: APP_ID,
      removed: "app.acme.com",
      id: "dom-1"
    });
  });

  it("exits NOT_FOUND without deleting for a domain the app does not have", async () => {
    stubFetch(routes({ [`GET ${DOMAINS}`]: list }), calls);

    const failed = await failure(runAppsDomainsRemove(fakeContext(), APP, "nope.acme.com", false));

    expect(failed.code).toBe(ExitCode.NOT_FOUND);
    expect(calls.some((c) => c.startsWith("DELETE"))).toBe(false);
  });

  it("rejects with the status's exit code when the delete fails", async () => {
    stubFetch(
      routes({
        [`GET ${DOMAINS}`]: list,
        [`DELETE ${DOMAINS}/dom-1`]: () => ({ status: 503, body: { error: "unavailable" } })
      }),
      calls
    );

    const failed = await failure(runAppsDomainsRemove(fakeContext(), APP, "app.acme.com", false));

    expect(failed.code).toBe(ExitCode.UNAVAILABLE);
  });
});
