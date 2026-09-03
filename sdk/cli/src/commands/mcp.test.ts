/**
 * The MCP server, driven over real stdio.
 *
 * Spawned rather than unit-tested because the thing worth checking is the
 * PROTOCOL: that the handshake completes, that stdout carries frames and
 * nothing else, and that a failing tool call comes back as a result the model
 * can read rather than as a dead transport.
 */

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

const BIN = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "dist", "main.mjs");

interface Frame {
  id?: number;
  result?: Record<string, unknown>;
  error?: Record<string, unknown>;
}

/**
 * Send a batch of JSON-RPC requests and collect the frames that come back.
 *
 * Points every credential and cache lookup at a path that does not exist, so
 * nothing here can read a developer's real token or reach a real deployment —
 * the tool calls under test are the ones that fail before any network I/O.
 */
async function rpc(requests: object[], timeoutMs = 20_000, cacheDir?: string): Promise<Frame[]> {
  if (!existsSync(BIN)) throw new Error(`${BIN} missing — run \`pnpm build\``);

  const child = spawn(process.execPath, [BIN, "mcp", "--env", "production"], {
    env: {
      ...process.env,
      OXY_CREDENTIALS_PATH: join(BIN, "..", "__no_creds__.json"),
      OXYC_CACHE_DIR: cacheDir ?? join(BIN, "..", "__no_cache__"),
      OXY_TOKEN: "",
      NO_COLOR: "1"
    }
  });

  let out = "";
  child.stdout.on("data", (d) => {
    out += d;
  });
  for (const req of requests) child.stdin.write(`${JSON.stringify(req)}\n`);
  child.stdin.end();

  await Promise.race([
    new Promise<void>((r) => child.on("close", () => r())),
    new Promise<void>((r) =>
      setTimeout(() => {
        child.kill();
        r();
      }, timeoutMs)
    )
  ]);

  return out
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as Frame);
}

/**
 * A cache directory holding a ready route catalog for `--env production`.
 *
 * `loadCatalog` reads this before it reaches for the network and returns it
 * whole while it is inside the TTL, so the refusal paths below are exercised
 * against a real catalog with no deployment and no credential in sight. The
 * file layout is `catalog/<hostKey>.json` with the host stored inside it —
 * `readDisk` rejects a file whose stored host disagrees with the target.
 */
function seedCatalog(routes: Array<{ path: string; description?: string }>): string {
  const dir = mkdtempSync(join(tmpdir(), "oxyc-mcp-"));
  SCRATCH.push(dir);
  mkdirSync(join(dir, "catalog"), { recursive: true });
  writeFileSync(
    join(dir, "catalog", "app.oxygen-hq.com.json"),
    JSON.stringify({
      host: "app.oxygen-hq.com",
      fetchedAt: Date.now(),
      surfaces: [{ id: "workspace", label: "Workspace", credential: "bearer" }],
      routes: routes.map((r) => ({
        method: "GET",
        path: r.path,
        surface: "workspace",
        credential: "bearer",
        path_parameters: [],
        description: r.description ?? "",
        note: "",
        handler: "handler",
        role: "fleet-ok"
      }))
    })
  );
  return dir;
}

/** `n` routes that all carry the same substring, so one filter matches them all. */
function manyRoutes(n: number): Array<{ path: string; description: string }> {
  return Array.from({ length: n }, (_, i) => ({
    path: `/api/thing-${i}/details`,
    description: `endpoint number ${i} in the same family as every other one`
  }));
}

const SCRATCH: string[] = [];
afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
});

const INIT = {
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "test", version: "0" }
  }
};

describe("the MCP handshake", () => {
  it("initialises and names itself", async () => {
    const [init] = await rpc([INIT]);
    expect((init?.result?.serverInfo as { name?: string })?.name).toBe("oxyc");
  });
});

describe("the tool list", () => {
  it("exposes exactly the four tools", async () => {
    const frames = await rpc([INIT, { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }]);
    const tools = (frames.find((f) => f.id === 2)?.result?.tools ?? []) as { name: string }[];
    expect(tools.map((t) => t.name).sort()).toEqual([
      "oxy_request",
      "oxy_routes",
      "oxy_schema",
      "oxy_whoami"
    ]);
  });

  /**
   * THE DESIGN DECISION, PINNED. One tool per endpoint would mean ~670 tool
   * schemas, and an agent runtime ships every one of them in EVERY request —
   * tens of kilobytes of context spent per turn before a question is asked.
   * Four tools cost a couple of KB and reach every endpoint, including ones
   * added after this package shipped.
   *
   * The budget is what stops that erosion: adding a fifth tool is fine, adding
   * fifty is the thing this number exists to catch.
   */
  it("keeps the per-turn schema cost small — the reason there are four", async () => {
    const frames = await rpc([INIT, { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }]);
    const tools = frames.find((f) => f.id === 2)?.result?.tools;
    const bytes = JSON.stringify(tools).length;
    expect(bytes).toBeLessThan(8_000);
  });

  it("tells the model to discover before it guesses", async () => {
    const frames = await rpc([INIT, { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }]);
    const tools = (frames.find((f) => f.id === 2)?.result?.tools ?? []) as {
      name: string;
      description: string;
    }[];
    const routes = tools.find((t) => t.name === "oxy_routes");
    expect(routes?.description).toMatch(/before .*oxy_request|rather than guessing/i);
  });
});

describe("tool failures", () => {
  /**
   * A throw inside a handler would kill the session. The model can act on
   * "here is what went wrong"; it cannot act on a dead transport.
   */
  it("returns an unknown tool as a result, not as a broken stream", async () => {
    const frames = await rpc([
      INIT,
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "nope", arguments: {} } },
      { jsonrpc: "2.0", id: 3, method: "tools/list", params: {} }
    ]);
    const failed = frames.find((f) => f.id === 2);
    expect(failed?.result?.isError).toBe(true);
    // The session survived: a later request still gets an answer.
    expect(frames.find((f) => f.id === 3)?.result?.tools).toBeDefined();
  });

  /**
   * With no credential, a call that needs one must come back as a readable
   * error carrying the hint — the same `CliError` the CLI would have printed.
   */
  it("surfaces a missing credential as a readable error, with the hint", async () => {
    const frames = await rpc([
      INIT,
      {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: { name: "oxy_whoami", arguments: {} }
      }
    ]);
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBe(true);
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/not authenticated/i);
    expect(body).toMatch(/oxyc login/);
  });

  /** An unresolved placeholder is caught before any request is attempted. */
  it("refuses an unresolvable placeholder with the flag that would fill it", async () => {
    const frames = await rpc([
      INIT,
      {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: { name: "oxy_request", arguments: { path: "{workspace}/threads" } }
      }
    ]);
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBe(true);
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/could not resolve \{workspace\}/);
    expect(body).toMatch(/--workspace/);
  });
});

describe("the route cap", () => {
  /**
   * THE COUNT IS THE TEST, not whether a filter was supplied. `searchRoutes`
   * matches a substring of method, path, surface OR description, so a filter
   * like "e" or "api" narrows nothing — and an uncapped result ships every
   * matching route WITH its description straight into a context window. This
   * case is the whole reason the guard is not gated on `!filter`.
   */
  it("degrades a broad FILTER to method and path, rather than refusing it", async () => {
    const dir = seedCatalog(manyRoutes(200));
    const frames = await rpc(
      [
        INIT,
        {
          jsonrpc: "2.0",
          id: 2,
          method: "tools/call",
          params: { name: "oxy_routes", arguments: { filter: "e" } }
        }
      ],
      20_000,
      dir
    );
    const result = frames.find((f) => f.id === 2)?.result;
    // NOT an error: "admin", "workspace" and "query" all clear 60 in good
    // faith, and "guess something narrower" is a dead end for a caller who
    // cannot see what it hit.
    expect(result?.isError).toBeFalsy();
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/200 routes match/);
    // It names the filter that failed to narrow, and how to get descriptions back.
    expect(body).toContain('"e"');
    expect(body).toMatch(/description/);

    // Split on the blank line every note ends with, not on the first "[" — a
    // note that ever contains a bracket would silently take the wrong slice.
    // Guarded for the no-note body, where `lastIndexOf` is -1 and a bare
    // `slice(+2)` would eat the leading bracket instead of failing.
    const payload = body.includes("\n\n") ? body.slice(body.lastIndexOf("\n\n") + 2) : body;
    const rows = JSON.parse(payload) as Record<string, unknown>[];
    expect(rows).toHaveLength(200);
    // DESCRIPTION is the only field dropped: `credential` is what separates the
    // bearer mount from the API-key one, and a caller needs it in either form.
    expect(Object.keys(rows[0] ?? {}).sort()).toEqual(["credential", "method", "path"]);
  });

  /**
   * Refusal is reserved for a match set too large even to list. At ~670 routes
   * a deployment-wide match is the only thing that reaches it.
   */
  it("still refuses a match set too large even to list", async () => {
    const dir = seedCatalog(manyRoutes(500));
    const frames = await rpc(
      [
        INIT,
        {
          jsonrpc: "2.0",
          id: 2,
          method: "tools/call",
          params: { name: "oxy_routes", arguments: { filter: "e" } }
        }
      ],
      20_000,
      dir
    );
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBe(true);
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/500 routes match/);
    expect(body).toMatch(/narrower/i);
  });

  it("returns a genuinely narrow filter in full", async () => {
    const dir = seedCatalog([
      ...manyRoutes(200),
      { path: "/api/{workspace}/threads", description: "list the threads" }
    ]);
    const frames = await rpc(
      [
        INIT,
        {
          jsonrpc: "2.0",
          id: 2,
          method: "tools/call",
          params: { name: "oxy_routes", arguments: { filter: "threads" } }
        }
      ],
      20_000,
      dir
    );
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBeFalsy();
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    // Under the threshold the description is present — that is the whole
    // difference between this and the degraded listing above.
    expect(JSON.parse(body)).toEqual([
      {
        method: "GET",
        path: "/api/{workspace}/threads",
        credential: "bearer",
        description: "list the threads"
      }
    ]);
  });

  /**
   * An empty array reads as "this deployment has no such endpoint", which is
   * usually false — so a zero match says what to do instead.
   */
  it("explains a zero match rather than returning []", async () => {
    const dir = seedCatalog([{ path: "/api/{workspace}/threads" }]);
    const frames = await rpc(
      [
        INIT,
        {
          jsonrpc: "2.0",
          id: 2,
          method: "tools/call",
          params: { name: "oxy_routes", arguments: { filter: "zzzznope" } }
        }
      ],
      20_000,
      dir
    );
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBe(true);
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/No route matches/);
    expect(body).toMatch(/all=true/);
  });
});

describe("required arguments", () => {
  /**
   * A model that omits `path` used to reach `request()` with `undefined` and
   * build a call to `/api/` — a 404 from the deployment, which reads as "that
   * endpoint does not exist" rather than "you forgot an argument".
   */
  it("names the missing argument instead of calling /api/", async () => {
    const frames = await rpc([
      INIT,
      {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: { name: "oxy_request", arguments: {} }
      }
    ]);
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBe(true);
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/oxy_request needs path/);
  });

  /** An empty string is as missing as an absent key, and easier to send. */
  it("treats a blank string as missing", async () => {
    const frames = await rpc([
      INIT,
      {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: { name: "oxy_schema", arguments: { path: "   " } }
      }
    ]);
    const result = frames.find((f) => f.id === 2)?.result;
    expect(result?.isError).toBe(true);
    const body = ((result?.content ?? []) as { text: string }[])[0]?.text ?? "";
    expect(body).toMatch(/oxy_schema needs path/);
  });
});
