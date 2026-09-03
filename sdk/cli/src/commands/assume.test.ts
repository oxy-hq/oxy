/**
 * `oxyc assume`, driven through the real binary against a fake deployment.
 *
 * A LOCAL SERVER, not a mock, because what is under test is the wire contract
 * with `/api/assume` — the body `start` sends, the query `end` sends, and the
 * status-to-reason mapping. A mock of the client would test the client's idea
 * of the server, which is the thing that drifts.
 */

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { ExitCode } from "../util/errors.js";
import { orgOrHint } from "./assume.js";

const BIN = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "dist", "main.mjs");
const SCRATCH: string[] = [];
afterAll(() => {
  for (const dir of SCRATCH) rmSync(dir, { recursive: true, force: true });
});

/** What the fake deployment last received, so a test can assert the wire. */
interface Seen {
  method?: string;
  url?: string;
  body?: string;
}

const ORG_ID = "11111111-2222-3333-4444-555555555555";

let server: Server;
let base: string;
let seen: Seen;
let assumeStatus = 200;
/** When true, `/api/orgs` and `/api/admin/orgs-meta` 403 — the partner's view. */
let partnerOnly = false;

beforeAll(async () => {
  server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => {
      body += c;
    });
    req.on("end", () => {
      const url = req.url ?? "";
      if (url.startsWith("/api/orgs") || url.startsWith("/api/admin/orgs-meta")) {
        if (partnerOnly) {
          res.writeHead(403);
          return res.end("{}");
        }
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify([{ slug: "acme", id: ORG_ID }]));
      }
      // The partner surface is TWO hops, and the first partner holds nothing —
      // so the walk has to continue rather than stop at the first miss.
      if (url === "/api/partners") {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(
          JSON.stringify([{ partner_id: "empty-partner" }, { partner_id: "real-partner" }])
        );
      }
      if (url === "/api/partners/empty-partner/orgs") {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify([{ slug: "someone-else", id: "0" }]));
      }
      if (url === "/api/partners/real-partner/orgs") {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify([{ slug: "acme", org_id: ORG_ID }]));
      }
      if (url.startsWith("/api/assume/current")) {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(
          JSON.stringify([
            {
              id: "s1",
              org_id: ORG_ID,
              org_name: "Acme",
              org_slug: "acme",
              is_partner: false,
              actor_email: "staff@oxy.tech",
              reason: "triage #123",
              started_at: new Date().toISOString(),
              expires_at: new Date(Date.now() + 42 * 60_000).toISOString()
            }
          ])
        );
      }
      if (url.startsWith("/api/assume")) {
        seen = { method: req.method, url, body };
        if (assumeStatus !== 200) {
          res.writeHead(assumeStatus);
          return res.end("{}");
        }
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(
          JSON.stringify({
            id: "s1",
            org_id: ORG_ID,
            org_name: "Acme",
            org_slug: "acme",
            is_partner: false,
            actor_email: "staff@oxy.tech",
            reason: "triage #123",
            started_at: new Date().toISOString(),
            expires_at: new Date(Date.now() + 3_600_000).toISOString()
          })
        );
      }
      res.writeHead(404);
      res.end("{}");
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});
afterAll(() => server.close());

/**
 * ASYNC, and that is not a style preference. `spawnSync` blocks this process's
 * event loop, so the loopback server above — which lives in this process —
 * could never answer the child, and every request sat until the client
 * timeout. The first version of this file hung for ten minutes. The same
 * mistake, with the same cause, is written down in `cache-permissions.test.ts`.
 */
function oxyc(...args: string[]): Promise<{ status: number; stdout: string; stderr: string }> {
  if (!existsSync(BIN)) throw new Error(`${BIN} missing — run \`pnpm build\``);
  const home = mkdtempSync(join(tmpdir(), "oxyc-assume-"));
  SCRATCH.push(home);
  return new Promise((done) => {
    const child = spawn(process.execPath, [BIN, ...args, "--target", base], {
      env: {
        ...process.env,
        HOME: home,
        OXY_CREDENTIALS_PATH: join(home, "credentials.json"),
        OXYC_CACHE_DIR: join(home, "cache"),
        OXY_TOKEN: "test-token",
        NO_COLOR: "1"
      }
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => {
      stdout += d;
    });
    child.stderr.on("data", (d) => {
      stderr += d;
    });
    child.on("close", (status) => done({ status: status ?? -1, stdout, stderr }));
  });
}

describe("login's flag combinations", () => {
  /**
   * ALL OF THESE REFUSE BEFORE A BROWSER OPENS, which is the property under
   * test — not the message. Two of them used to be checked inside `runLogin`,
   * after every env had been through its flow: `--login-env staging --assume
   * acme -r why` opened two browsers, waited for two callbacks, and only then
   * exited USAGE. A timeout is the assertion that would catch a regression, so
   * each of these completes in milliseconds precisely because nothing opens.
   */
  it.each([
    [["login", "--assume", "acme"], /--assume requires --reason/],
    [["login", "-r", "why"], /--reason is only valid with --assume/],
    [
      ["login", "--login-env", "staging", "--assume", "acme", "-r", "why"],
      /--assume is only valid when logging into a single env/
    ],
    // THIS ROW FIRES FOR A REASON ITS LABEL DOES NOT NAME: `oxyc()` appends
    // `--target` to every call, so the case supplies the multi-env half and
    // the harness supplies the target. The refusal is real; the row is not
    // evidence that a caller typing only `--login-env staging` is refused —
    // and by the same appendage, a WORKING multi-env login cannot be driven
    // here at all. The three rows above are genuine.
    [["login", "--login-env", "staging"], /--target is only valid when logging into a single env/]
  ])("refuses %j without doing any work", async (args, message) => {
    const r = await oxyc(...(args as string[]));
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(message as RegExp);
  });
});

describe("the org a verb is about", () => {
  const withSlug = (orgSlug?: string) =>
    ({ env: () => ({ target: "https://x", orgSlug }) }) as never;

  /**
   * THE FALLBACK BOTH VERBS SHARE, pinned here because it cannot be reached
   * end-to-end: every loopback call passes `--target`, which overrides `--env`
   * and takes its org slug with it. `end` not having this fallback is what
   * made `oxyc assume end --env https://acme.oxygen-hq.com` end EVERY live
   * session instead of one.
   */
  it("prefers what was passed, and falls back to what the env named", () => {
    expect(orgOrHint(withSlug("from-env"), "explicit")).toBe("explicit");
    expect(orgOrHint(withSlug("from-env"), undefined)).toBe("from-env");
    expect(orgOrHint(withSlug(undefined), undefined)).toBe("");
    // Whitespace is not an org — `end` treats "" as "no org named".
    expect(orgOrHint(withSlug(undefined), "  ")).toBe("");
    // WHAT THIS DOES NOT PIN: that `end` calls it. Nothing in a loopback
    // harness can see that, for the `--target` reason above — a mutation
    // replacing `orgOrHint(ctx, org)` with `org` inside `runAssumeEnd` passes
    // this suite. The expression is guarded; its second caller is not.
  });
});

describe("assume start", () => {
  it("resolves a slug to an id and posts it with the reason", async () => {
    assumeStatus = 200;
    const r = await oxyc("assume", "start", "--org", "acme", "-r", "triage #123");
    expect(r.status, r.stderr).toBe(0);
    expect(seen.method).toBe("POST");
    // THE WIRE, not the client's idea of it: `/api/assume` takes an org UUID,
    // so a slug that reached the server unresolved would 404 in production.
    expect(JSON.parse(seen.body ?? "{}")).toEqual({ org_id: ORG_ID, reason: "triage #123" });
    expect(r.stdout).toMatch(/Now acting as Acme \(acme\)/);
  });

  it("refuses an empty reason before making a request", async () => {
    const r = await oxyc("assume", "start", "--org", "acme", "-r", "   ");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/--reason must not be empty/);
    expect(r.stderr).toMatch(/impersonation log/);
  });

  it("says WHY a 403 happened rather than leaving a bare status", async () => {
    assumeStatus = 403;
    const r = await oxyc("assume", "start", "--org", "acme", "-r", "triage");
    expect(r.status).not.toBe(0);
    expect(r.stderr).toMatch(/not allowed to act as this org/);
    expect(r.stderr).toMatch(/partner only as an assigned client/);
    assumeStatus = 200;
  });

  /**
   * THE TIER THAT WAS DEAD. The first version called `/api/partner/clients`,
   * which is not a route in this repo — so a partner, the one population this
   * fallback exists for, was told the org is not visible. Invisible to the
   * suite because `/api/orgs` answered first and the fallback never ran.
   */
  it("finds an org through the partner surface when the first two are closed", async () => {
    partnerOnly = true;
    const r = await oxyc("assume", "start", "--org", "acme", "-r", "triage");
    partnerOnly = false;
    expect(r.status, r.stderr).toBe(0);
    // `org_id` on a partner's client list, `id` on the other two.
    expect(JSON.parse(seen.body ?? "{}").org_id).toBe(ORG_ID);
  });

  /**
   * A URL is one of the three spellings of `--org`, and the same parse the
   * `--env` hint uses.
   *
   * THE HINT ITSELF CANNOT BE DRIVEN HERE, and that is a property of the
   * harness rather than a gap in the feature: every call adds `--target` to
   * reach the loopback, and `--target` overrides `--env` — including the org
   * slug, which `resolveEnv` reads off whichever URL won. So `--env
   * https://acme.oxygen-hq.com --target http://127.0.0.1:x` has no org to
   * hint. This covers the parse; `resolveOrgId` reaching for
   * `ctx.env().orgSlug` is one line above it and shared with `end`.
   */
  it("reads an org out of a URL passed to --org", async () => {
    const r = await oxyc("assume", "start", "-r", "triage", "--org", "https://acme.oxygen-hq.com");
    expect(r.status, r.stderr).toBe(0);
    expect(JSON.parse(seen.body ?? "{}").org_id).toBe(ORG_ID);
  });

  it("names the flag when no org can be found anywhere", async () => {
    const r = await oxyc("assume", "start", "-r", "triage");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/no organization given/);
    expect(r.stderr).toMatch(/--org/);
  });
});

describe("assume status", () => {
  it("reports a live session with the time left", async () => {
    const r = await oxyc("assume", "status");
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toMatch(/Acme \(acme\)/);
    expect(r.stdout).toMatch(/4[12]m/);
    expect(r.stdout).toMatch(/triage #123/);
  });
});

describe("assume end", () => {
  it("scopes to one org when given one", async () => {
    const r = await oxyc("assume", "end", "--org", "acme");
    expect(r.status, r.stderr).toBe(0);
    expect(seen.method).toBe("DELETE");
    expect(seen.url).toContain(`org_id=${ORG_ID}`);
  });

  /**
   * No `--org` ends everything, which is what the Rust does — `--all` exists to
   * let you SAY so, not to unlock it. Asserted because the opposite (refusing
   * without a flag) is the reasonable-looking behaviour someone would add.
   */
  /**
   * THE DESTRUCTIVE ONE, in the shape this harness can reach. `end` ignored the
   * org an `--env` URL names while `start` honoured it — so one URL meant an
   * org for one verb and nothing for the other, and `end` quietly ended EVERY
   * live session, including orgs an operator was mid-investigation in. Both
   * verbs now go through the same `org ?? ctx.env().orgSlug`.
   */
  it("scopes to an org given as a URL rather than ending everything", async () => {
    const r = await oxyc("assume", "end", "--org", "https://acme.oxygen-hq.com");
    expect(r.status, r.stderr).toBe(0);
    expect(seen.url).toContain(`org_id=${ORG_ID}`);
  });

  /**
   * AN EMPTY `--org` IS NOT AN ABSENT ONE. `--org "$ORG"` with `$ORG` unset
   * sent an unscoped DELETE — every live session, on a verb that cannot be
   * undone — and swallowed the `--env` hint on the way. Neither guard caught
   * it: `orgOrHint`'s `??` treats "" as a value, and the `--all`/`--org`
   * conflict check is falsy on "".
   */
  it("refuses an empty --org rather than ending everything", async () => {
    // Cleared first: `seen` is module state, so asserting on it without this
    // reads whatever the previous test left there.
    seen = {};
    const r = await oxyc("assume", "end", "--org", "");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/--org was given but is empty/);
    // And nothing reached the deployment — the refusal is before the request.
    expect(seen.method).toBeUndefined();
  });

  it("refuses a whitespace --org the same way", async () => {
    const r = await oxyc("assume", "end", "--org", "   ");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/--org was given but is empty/);
  });

  /**
   * `--all` and `--org` are REFUSED together rather than one silently winning.
   * The Rust declares `conflicts_with`; a destructive verb is the wrong place
   * to guess which of two contradictory things a caller meant.
   */
  it("refuses --all beside --org rather than picking one", async () => {
    const r = await oxyc("assume", "end", "--all", "--org", "https://acme.oxygen-hq.com");
    expect(r.status).toBe(ExitCode.USAGE);
    expect(r.stderr).toMatch(/--all and --org name different sets/);
  });

  it("ends every session when given no org", async () => {
    const r = await oxyc("assume", "end");
    expect(r.status, r.stderr).toBe(0);
    expect(seen.method).toBe("DELETE");
    expect(seen.url).not.toContain("org_id");
  });
});
