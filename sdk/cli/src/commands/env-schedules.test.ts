/**
 * `oxyc env schedules` — an environment's schedules switch, driven with
 * `globalThis.fetch` stubbed (`testing/stub-fetch.ts`).
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import type { Context } from "../context/resolve.js";
import { stubFetch } from "../testing/stub-fetch.js";
import { CliError, ExitCode } from "../util/errors.js";
import { describeSchedules, envSchedules, runEnvSchedules } from "./env-schedules.js";

const TARGET = "https://oxy.test";
const APP_ID = "a1a1a1a1-2222-3333-4444-555555555555";
const SWITCH = `/api/customer-apps/${APP_ID}/environments/dev-a1/schedules`;

function fakeContext(bearer = "tok"): Context {
  return {
    cwd: "/tmp",
    flags: { env: "production", apiKeyEnv: "OXY_API_KEY" },
    target: () => TARGET,
    env: () => ({ target: TARGET, orgSlug: undefined }) as ReturnType<Context["env"]>,
    bearer: async () => bearer,
    maybeBearer: async () => bearer,
    storedBearer: () => bearer,
    async credential() {
      return { token: bearer, source: "env" };
    },
    serviceAccount: () => undefined,
    apiKey: () => undefined,
    customer: () => undefined,
    repoDir: () => undefined,
    placeholders: () => ({}),
    withEnv: () => fakeContext(bearer)
  };
}

const APPS = {
  "GET /api/customer-apps?limit=100&offset=0": () => ({
    status: 200,
    body: { items: [{ id: APP_ID, slug: "store", org_slug: "acme" }], next_offset: null }
  })
};

const ON = {
  environment: "dev-a1",
  enabled: true,
  schedules: [
    {
      id: "s-1",
      function: "tick",
      cron_expr: "0 * * * *",
      timezone: "UTC",
      next_run_at: "2026-10-17T10:00:00Z",
      last_fired_at: null,
      last_run_id: null,
      last_error: null
    }
  ],
  warnings: []
};

afterEach(() => vi.unstubAllGlobals());

describe("envSchedules", () => {
  it("PUTs {enabled} to flip the switch and returns the read-back", async () => {
    const calls = stubFetch(TARGET, {
      ...APPS,
      [`PUT ${SWITCH}`]: () => ({ status: 200, body: ON })
    });
    const shown = await envSchedules(fakeContext(), "acme/store", "dev-a1", true);
    expect(shown).toEqual(ON);
    expect(calls.find((c) => c.method === "PUT")?.body).toBe('{"enabled":true}');
  });

  it("reads the switch with a GET when neither flag is given", async () => {
    const calls = stubFetch(TARGET, {
      ...APPS,
      [`GET ${SWITCH}`]: () => ({ status: 200, body: { ...ON, enabled: false, schedules: [] } })
    });
    let printed = "";
    const write = vi.spyOn(process.stdout, "write").mockImplementation((chunk) => {
      printed += String(chunk);
      return true;
    });
    await runEnvSchedules(fakeContext(), "acme/store", "dev-a1", { json: false });
    write.mockRestore();
    expect(printed).toBe("dev-a1 — schedules off\n");
    expect(calls.filter((c) => c.method !== "GET")).toEqual([]);
  });

  it("refuses production, a sandbox token and --on with --off before any request", async () => {
    const calls = stubFetch(TARGET, {});
    for (const run of [
      envSchedules(fakeContext(), "acme/store", "production", true),
      envSchedules(fakeContext("oxy_sbx_abc"), "acme/store", "dev-a1", true),
      runEnvSchedules(fakeContext(), "acme/store", "dev-a1", { on: true, off: true, json: false })
    ]) {
      const e = await run.catch((caught: unknown) => caught);
      expect(e).toBeInstanceOf(CliError);
      expect((e as CliError).code).toBe(ExitCode.USAGE);
    }
    expect(calls).toEqual([]);
  });
});

describe("describeSchedules", () => {
  it("names each row's cron and next fire, and says when an on switch has nothing to fire", () => {
    expect(describeSchedules(ON)).toEqual([
      "dev-a1 — schedules on",
      "  tick  0 * * * * (UTC), next 2026-10-17T10:00:00Z"
    ]);
    expect(describeSchedules({ ...ON, schedules: [], warnings: ["w"] })).toEqual([
      "dev-a1 — schedules on",
      "  no function in the build this environment serves declares a schedule",
      "  warning: w"
    ]);
  });
});
