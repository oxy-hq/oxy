/**
 * `oxyc fn dev --read-through` against a local fake deployment.
 *
 * The fake server records every request it receives. What these tests hold:
 *   - the three reads reach it, on the routes a published app uses, carrying
 *     the caller's bearer;
 *   - every write op the same handler makes is answered in memory and the
 *     server never sees it;
 *   - without the flag, nothing touches the network at all.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createContext } from "../context/resolve.js";
import type { Runner } from "../publish/functions.js";
import { ExitCode } from "../util/errors.js";
import { devOnce, readThroughNotice, renderReport } from "./fn-dev.js";
import { isolateColumnNames, READ_THROUGH_OPS } from "./fn-dev-read-through.js";

const PROJECT = "11111111-2222-4333-8444-555555555555";
const BEARER = "oxy_pat_caller_token";

const MANIFEST = {
  schemaVersion: 2,
  slug: "store",
  orgSlug: "acme",
  name: "Store",
  functions: {
    work: {
      route: true,
      destinations: ["wh"],
      email: { send: true },
      storage: { write: true }
    }
  }
};

interface Seen {
  method: string;
  url: string;
  authorization: string | undefined;
  body: unknown;
}

let dir: string;
let server: Server;
let target: string;
let seen: Seen[];
let status: number;

beforeEach(async () => {
  dir = mkdtempSync(join(tmpdir(), "oxyc-fn-dev-rt-"));
  writeFileSync(join(dir, "oxy-app.json"), JSON.stringify(MANIFEST));
  seen = [];
  status = 200;
  server = createServer((req, res) => {
    let raw = "";
    req.on("data", (chunk) => {
      raw += chunk;
    });
    req.on("end", () => {
      seen.push({
        method: req.method ?? "",
        url: req.url ?? "",
        authorization: req.headers.authorization,
        body: raw ? JSON.parse(raw) : undefined
      });
      res.setHeader("content-type", "application/json");
      res.statusCode = status;
      if (status !== 200) {
        res.end(JSON.stringify({ message: "not a member of this project" }));
      } else if (req.url?.endsWith("/semantic-query")) {
        res.end(
          JSON.stringify({ columns: ["region", "revenue"], rows: [["west", 7]], truncated: false })
        );
      } else {
        res.end(
          JSON.stringify({
            columns: ["id", "name"],
            rows: [
              [1, "Ada"],
              [2, "Grace"]
            ],
            truncated: true
          })
        );
      }
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  target = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  vi.stubEnv("OXY_TOKEN", BEARER);
  vi.stubEnv("OXY_PROJECT", "");
  vi.stubEnv("OXY_ORG", "");
});

afterEach(async () => {
  vi.unstubAllEnvs();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  rmSync(dir, { recursive: true, force: true });
});

function writing(source: string): Runner {
  return (_label, _command, args) => {
    const outfile = args.find((a) => a.startsWith("--outfile="))?.slice("--outfile=".length);
    if (!outfile) throw new Error("no --outfile in the esbuild argv");
    writeFileSync(outfile, source);
  };
}

/** Three reads, then every kind of write the handler can make, each caught so all are attempted. */
const HANDLER = `
export default async function (req, ctx) {
  const out = {};
  out.query = await ctx.query("SELECT id, name FROM people");
  out.warehouse = await ctx.warehouse.query("wh", "SELECT 1");
  out.semantic = await ctx.semantic.query({ topic: "sales", dimensions: ["store.region"], measures: ["sales.revenue"], scope: "reach" });
  const attempts = {
    insert: () => ctx.warehouse.insert("wh", "t", [{ a: 1 }]),
    exec: () => ctx.warehouse.exec("wh", "DELETE FROM t"),
    email: () => ctx.email.send({ to: ctx.user.email, subject: "s", html: "<p>h</p>" }),
    storage: () => ctx.storage.put("a.txt", "x"),
    fetch: () => ctx.fetch("https://example.com/hook", { method: "POST", body: "{}" }),
    oltp: () => ctx.oltp.exec("DELETE FROM t", []),
    stream: async () => { for await (const _ of ctx.queryStream("SELECT 1")) {} },
  };
  out.writes = {};
  for (const [name, run] of Object.entries(attempts)) {
    try { await run(); out.writes[name] = "ok"; } catch (e) { out.writes[name] = "threw"; }
  }
  return Response.json(out);
}`;

function context() {
  return createContext({ target, project: PROJECT }, dir);
}

describe("fn dev --read-through", () => {
  it("sends the three reads to the deployment with the caller's bearer, on the app's routes", async () => {
    const result = await devOnce(
      "work",
      { readThrough: true },
      { cwd: dir, run: writing(HANDLER), context: context() }
    );

    expect(result.error).toBeUndefined();
    expect(seen.map((s) => `${s.method} ${s.url}`)).toEqual([
      `POST /api/projects/${PROJECT}/query`,
      `POST /api/projects/${PROJECT}/query`,
      `POST /api/projects/${PROJECT}/semantic-query`
    ]);
    expect(seen.every((s) => s.authorization === `Bearer ${BEARER}`)).toBe(true);
    expect(seen[0]?.body).toEqual({ sql: "SELECT id, name FROM people" });
    expect(seen[1]?.body).toEqual({ sql: "SELECT 1", database: "wh" });
    expect(seen[2]?.body).toEqual({
      topic: "sales",
      dimensions: ["store.region"],
      measures: ["sales.revenue"],
      scope: "reach",
      v: 1
    });

    // The handler got what the host ops answer: rows as objects, `truncated` kept.
    const body = JSON.parse(result.body ?? "{}") as Record<string, unknown>;
    expect(body.query).toEqual({
      rows: [
        { id: 1, name: "Ada" },
        { id: 2, name: "Grace" }
      ],
      truncated: true
    });
    // The route strips airlayer's `view__member` prefix; the isolate does not. The
    // keys are put back from the spec so the handler sees what it will see published.
    expect(body.semantic).toEqual({
      rows: [{ store__region: "west", sales__revenue: 7 }],
      truncated: false
    });
  });

  it("answers every write in memory: the server sees no write, fetch, email or storage call", async () => {
    const result = await devOnce(
      "work",
      { readThrough: true },
      { cwd: dir, run: writing(HANDLER), context: context() }
    );

    const body = JSON.parse(result.body ?? "{}") as { writes: Record<string, string> };
    // The writes the testing context takes ran against its stores; the ones it
    // refuses or has no answer for threw there. None reached the fake server.
    expect(body.writes).toMatchObject({
      insert: "ok",
      exec: "ok",
      email: "ok",
      storage: "ok",
      fetch: "threw"
    });
    expect(seen).toHaveLength(3);
    expect(seen.every((s) => /\/(semantic-)?query$/.test(s.url))).toBe(true);

    const real = result.calls
      .filter((c) => READ_THROUGH_OPS.includes(c.op as never))
      .map((c) => c.op);
    expect(real).toEqual(["query", "warehouse.query", "semantic.query"]);
    for (const op of ["warehouse.insert", "warehouse.exec", "email.send", "storage.put", "fetch"]) {
      expect(result.ops).toContain(op);
    }
    // `queryStream` is a read, but not one of the three: it stays in memory.
    expect(result.ops).toContain("query_stream");
  });

  it("marks each call real or stubbed in the report and still says it is not the isolate", async () => {
    const result = await devOnce(
      "work",
      { readThrough: true },
      { cwd: dir, run: writing(HANDLER), context: context() }
    );
    const report = renderReport(result);
    expect(report).toMatch(/\n {2}query {2}ok {2}real {2}\{"sql":"SELECT id, name FROM people"\}/);
    expect(report).toMatch(/\n {2}warehouse\.query {2}ok {2}real /);
    expect(report).toMatch(/\n {2}semantic\.query {2}ok {2}real /);
    expect(report).toMatch(/\n {2}warehouse\.insert {2}ok {2}stubbed /);
    expect(report).toMatch(/\n {2}email\.send {2}ok {2}stubbed /);
    expect(report).toMatch(/\n {2}fetch {2}\S+ {2}stubbed /);
    expect(report).toContain(`read-through: query, warehouse.query, semantic.query → ${target}`);
    expect(readThroughNotice(target)).toMatch(/^LOCAL RUN, not the isolate/);
    expect(readThroughNotice(target)).toContain("stubbed");
  });

  it("touches no network without the flag", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    const result = await devOnce(
      "work",
      {},
      {
        cwd: dir,
        run: writing(
          "export default async (req, ctx) => { try { await ctx.query('SELECT 1'); } catch {} return new Response('ok'); }"
        ),
        // Even handed a context, a run without the flag never uses it.
        context: context()
      }
    );
    expect(result.ok).toBe(true);
    expect(result.readThrough).toBeUndefined();
    expect(seen).toEqual([]);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(renderReport(result)).toMatch(/\n {2}query {2}\S+ {2}stubbed /);
    fetchSpy.mockRestore();
  });

  it("surfaces the server's refusal to the handler; the gate is the server's", async () => {
    status = 403;
    const result = await devOnce(
      "work",
      { readThrough: true },
      {
        cwd: dir,
        run: writing(
          "export default async (req, ctx) => Response.json(await ctx.query('SELECT 1'));"
        ),
        context: context()
      }
    );
    expect(result.ok).toBe(false);
    expect(result.error).toContain("ctx.query: 403");
    expect(result.error).toContain("not a member of this project");
    expect(result.calls[0]).toMatchObject({ op: "query", outcome: "threw" });
  });

  it("fails the command, before the handler runs, with no credential", async () => {
    vi.stubEnv("OXY_TOKEN", "");
    const ctx = createContext(
      { target, project: PROJECT, tokenEnv: "OXYC_TEST_NO_SUCH_TOKEN" },
      dir
    );
    await expect(
      devOnce(
        "work",
        { readThrough: true },
        { cwd: dir, run: writing("throw new Error('handler ran')"), context: ctx }
      )
    ).rejects.toMatchObject({ code: ExitCode.AUTH });
    expect(seen).toEqual([]);
  });

  it("refuses a sandbox agent token, as `oxyc api` does", async () => {
    vi.stubEnv("OXY_TOKEN", "oxy_sbx_abcdef");
    await expect(
      devOnce(
        "work",
        { readThrough: true },
        { cwd: dir, run: writing(HANDLER), context: context() }
      )
    ).rejects.toMatchObject({ code: ExitCode.USAGE });
    expect(seen).toEqual([]);
  });

  it("looks the project up from the app's org and slug when none is pinned", async () => {
    const lookups: string[] = [];
    const original = server.listeners("request")[0] as (...a: unknown[]) => void;
    server.removeAllListeners("request");
    server.on("request", (req, res) => {
      if (req.url?.startsWith("/api/apps/")) {
        lookups.push(req.url);
        res.setHeader("content-type", "application/json");
        res.end(JSON.stringify({ project_id: PROJECT }));
        return;
      }
      original(req, res);
    });
    const result = await devOnce(
      "work",
      { readThrough: true },
      {
        cwd: dir,
        run: writing(
          "export default async (req, ctx) => Response.json(await ctx.query('SELECT 1'));"
        ),
        context: createContext({ target }, dir)
      }
    );
    expect(lookups).toEqual(["/api/apps/acme/store/build-config"]);
    expect(seen.map((s) => s.url)).toEqual([`/api/projects/${PROJECT}/query`]);
    expect(result.readThrough?.project).toBe(PROJECT);
  });

  it("needs a context: the flag without a deployment is a usage error", async () => {
    await expect(
      devOnce("work", { readThrough: true }, { cwd: dir, run: writing(HANDLER) })
    ).rejects.toMatchObject({ code: ExitCode.USAGE });
  });
});

describe("isolateColumnNames", () => {
  const spec = {
    dimensions: ["store.region", "store.city"],
    measures: ["sales.revenue"],
    time_dimensions: [{ dimension: "sales.created_at", granularity: "month" }]
  };

  it("puts back the view prefix the route strips, from the spec", () => {
    expect(isolateColumnNames(["region", "city", "revenue", "created_at"], spec)).toEqual([
      "store__region",
      "store__city",
      "sales__revenue",
      "sales__created_at"
    ]);
  });

  it("leaves a column the route kept qualified, or the spec does not name, alone", () => {
    expect(isolateColumnNames(["a__id", "b__id", "other"], spec)).toEqual([
      "a__id",
      "b__id",
      "other"
    ]);
  });

  it("does not guess when two views in the spec share the member name", () => {
    expect(
      isolateColumnNames(["id"], { dimensions: ["orders.id"], measures: ["users.id"] })
    ).toEqual(["id"]);
  });

  it("tolerates a spec with no members", () => {
    expect(isolateColumnNames(["x"], {})).toEqual(["x"]);
  });
});
