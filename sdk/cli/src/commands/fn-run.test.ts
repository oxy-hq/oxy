/**
 * `oxyc fn run` — Run now, and `--as-schedule`, driven with
 * `globalThis.fetch` stubbed (`testing/stub-fetch.ts`).
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import type { Context } from "../context/resolve.js";
import { stubFetch } from "../testing/stub-fetch.js";
import { CliError, ExitCode } from "../util/errors.js";
import { fnRun } from "./fn-run.js";

const TARGET = "https://oxy.test";
const APP_ID = "a1a1a1a1-2222-3333-4444-555555555555";
const RUNS = `/api/customer-apps/${APP_ID}/functions/tick/runs`;
const DETAIL = `/api/customer-apps/${APP_ID}/function-runs/r-1`;

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
  "GET /api/admin/apps?limit=100&offset=0": () => ({
    status: 200,
    body: { items: [{ id: APP_ID, slug: "store", org_slug: "acme" }], next_offset: null }
  })
};

const DONE = {
  status: "done",
  trigger: "schedule_now",
  answer: '{"channel":"dev-a1"}',
  invocation_id: "inv-1"
};

afterEach(() => vi.unstubAllGlobals());

describe("fnRun", () => {
  it("queues the run as the schedule, with no body, and reports its verdict", async () => {
    const calls = stubFetch(TARGET, {
      ...APPS,
      [`POST ${RUNS}?environment=dev-a1&as=schedule`]: () => ({
        status: 200,
        body: { run_id: "r-1", environment: "dev-a1" }
      }),
      [`GET ${DETAIL}?environment=dev-a1`]: () => ({ status: 200, body: DONE })
    });
    const result = await fnRun(fakeContext(), "acme/store", "tick", {
      appEnv: "dev-a1",
      asSchedule: true,
      timeoutSeconds: 5,
      pollMs: 1
    });
    expect(result).toEqual({
      function: "tick",
      environment: "dev-a1",
      runId: "r-1",
      status: "done",
      ok: true,
      trigger: "schedule_now",
      answer: { channel: "dev-a1" },
      error: undefined,
      invocationId: "inv-1"
    });
    const post = calls.find((c) => c.method === "POST");
    expect(post?.body).toBeUndefined();
  });

  it("is a production Run now with the input as the body, when nothing is named", async () => {
    const calls = stubFetch(TARGET, {
      ...APPS,
      [`POST ${RUNS}`]: () => ({ status: 200, body: { run_id: "r-1", environment: "production" } }),
      [`GET ${DETAIL}`]: () => ({ status: 200, body: { status: "failed", error: "boom" } })
    });
    const result = await fnRun(fakeContext(), "acme/store", "tick", {
      data: '{"store":7}',
      timeoutSeconds: 5,
      pollMs: 1
    });
    expect(result.ok).toBe(false);
    expect(result.error).toBe("boom");
    expect(calls.find((c) => c.method === "POST")?.body).toBe('{"store":7}');
  });

  it("carries the server's not_scheduled refusal", async () => {
    stubFetch(TARGET, {
      ...APPS,
      [`POST ${RUNS}?as=schedule`]: () => ({
        status: 409,
        body: { error: "not_scheduled", message: "function 'tick' declares no `schedule`" }
      })
    });
    const refused = await fnRun(fakeContext(), "acme/store", "tick", {
      asSchedule: true,
      timeoutSeconds: 5
    }).catch((e: unknown) => e);
    expect(refused).toBeInstanceOf(CliError);
    expect((refused as CliError).code).toBe(ExitCode.REQUEST);
  });

  it("refuses --data with --as-schedule, and a sandbox token outside its sandbox, before any request", async () => {
    const calls = stubFetch(TARGET, {});
    const usage = async (run: Promise<unknown>) => {
      const e = await run.catch((caught: unknown) => caught);
      expect(e).toBeInstanceOf(CliError);
      expect((e as CliError).code).toBe(ExitCode.USAGE);
    };
    await usage(
      fnRun(fakeContext(), "acme/store", "tick", {
        asSchedule: true,
        data: "{}",
        timeoutSeconds: 5
      })
    );
    await usage(
      fnRun(fakeContext("oxy_sbx_abc"), "acme/store", "tick", {
        asSchedule: true,
        appEnv: "staging",
        timeoutSeconds: 5
      })
    );
    await usage(fnRun(fakeContext(), "acme/store", "tick", { timeoutSeconds: 0 }));
    expect(calls).toEqual([]);
  });
});
