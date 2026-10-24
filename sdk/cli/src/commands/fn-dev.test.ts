/**
 * `oxyc fn dev`, end to end up to the bundler: a real app directory, the real
 * `createTestContext`, and a stand-in for the `pnpm exec esbuild` call (which
 * `bundleFunctions` takes as an injectable `Runner`) that writes a hand-written
 * ESM handler where esbuild would. The run against the template's own handler
 * and the real esbuild is in `scripts/ci/verify-templates.mjs`.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Runner } from "../publish/functions.js";
import { CliError, ExitCode } from "../util/errors.js";
import { devOnce, LOCAL_RUN_NOTICE, renderReport, runFnDev } from "./fn-dev.js";

const MANIFEST = {
  schemaVersion: 2,
  slug: "store",
  name: "Store",
  env: { STRIPE_KEY: { required: true } },
  functions: {
    greet: { route: true, email: { send: true } },
    plain: { route: true }
  }
};

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "oxyc-fn-dev-"));
  writeFileSync(join(dir, "oxy-app.json"), JSON.stringify(MANIFEST));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** A Runner that writes `source` to the `--outfile=` esbuild would have been given. */
function writing(source: string): Runner {
  return (_label, _command, args) => {
    const outfile = args.find((a) => a.startsWith("--outfile="))?.slice("--outfile=".length);
    if (!outfile) throw new Error("no --outfile in the esbuild argv");
    writeFileSync(outfile, source);
  };
}

const GREET = `
export default async function (req, ctx) {
  const { name } = JSON.parse(req.body);
  await ctx.email.send({ to: ctx.user.email, subject: "Hi " + name, html: "<p>hi</p>" });
  return Response.json({ ok: true, key: ctx.env.STRIPE_KEY ?? null });
}`;

describe("devOnce", () => {
  it("runs the handler on the manifest's context and records its host ops", async () => {
    writeFileSync(join(dir, ".env"), "STRIPE_KEY=sk_test_1\n");
    const result = await devOnce(
      "greet",
      { input: '{"name":"Ada"}' },
      { cwd: dir, run: writing(GREET) }
    );

    expect(result.ok).toBe(true);
    expect(result.status).toBe(200);
    expect(JSON.parse(result.body ?? "")).toEqual({ ok: true, key: "sk_test_1" });
    expect(result.ops).toEqual(["email.send"]);
    expect(result.calls[0]).toMatchObject({
      op: "email.send",
      outcome: "ok",
      args: { subject: "Hi Ada" }
    });
    expect(result.unsetRequired).toEqual([]);
  });

  it("defaults the body to {} and reports a required secret the env file lacks", async () => {
    const result = await devOnce(
      "plain",
      {},
      { cwd: dir, run: writing("export default async (req) => Response.json({ body: req.body });") }
    );
    expect(JSON.parse(result.body ?? "")).toEqual({ body: "{}" });
    expect(result.unsetRequired).toEqual(["STRIPE_KEY"]);
  });

  it("refuses where the host does: the manifest does not grant email.send to `plain`", async () => {
    const result = await devOnce(
      "plain",
      {},
      {
        cwd: dir,
        run: writing(
          "export default async (req, ctx) => { await ctx.email.send({ to: 'a@b.test', subject: 's', html: 'h' }); return new Response('sent'); }"
        )
      }
    );
    expect(result.ok).toBe(false);
    expect(result.error).toBeDefined();
    expect(result.calls[0]).toMatchObject({ op: "email.send", outcome: "refused" });
  });

  it("scores a non-2xx answer as a failure, like `fn call`", async () => {
    const result = await devOnce(
      "plain",
      {},
      { cwd: dir, run: writing("export default async () => new Response('no', { status: 403 });") }
    );
    expect(result).toMatchObject({ ok: false, status: 403, body: "no" });
    expect(result.error).toBeUndefined();
  });

  it("reports a handler that throws, with its stack", async () => {
    const result = await devOnce(
      "plain",
      {},
      { cwd: dir, run: writing("export default async () => { throw new Error('boom'); };") }
    );
    expect(result.ok).toBe(false);
    expect(result.error).toContain("boom");
  });

  it("is a usage error for a function the manifest does not declare", async () => {
    const run = writing(GREET);
    await expect(devOnce("nope", {}, { cwd: dir, run })).rejects.toMatchObject({
      code: ExitCode.USAGE,
      message: expect.stringContaining('declares no function "nope"')
    });
  });

  it("is a usage error outside an app directory", async () => {
    rmSync(join(dir, "oxy-app.json"));
    await expect(devOnce("greet", {}, { cwd: dir, run: writing(GREET) })).rejects.toMatchObject({
      code: ExitCode.USAGE
    });
  });

  it("passes the bundler's own failure through, not as a handler result", async () => {
    const failing: Runner = () => {
      throw new CliError("`pnpm exec esbuild` failed (exit 1)");
    };
    await expect(devOnce("greet", {}, { cwd: dir, run: failing })).rejects.toThrow(/esbuild/);
  });

  it("bundles with the argv publish uses", async () => {
    const argv: string[][] = [];
    await devOnce(
      "plain",
      {},
      {
        cwd: dir,
        run: (label, command, args, cwd) => {
          argv.push([label, command, ...args, cwd]);
          writing("export default async () => new Response('ok');")(label, command, args, cwd);
        }
      }
    );
    expect(argv[0]?.slice(0, 7)).toEqual([
      "fn:plain",
      "pnpm",
      "exec",
      "esbuild",
      "functions/plain.ts",
      "--bundle",
      "--format=esm"
    ]);
    expect(argv[0]).toContain("--platform=neutral");
    expect(argv[0]?.at(-1)).toBe(dir);
  });
});

describe("runFnDev and the report", () => {
  it("prints the response and the ops, and exits non-zero on a failed run", async () => {
    let printed = "";
    const deps = {
      cwd: dir,
      stdout: (text: string) => {
        printed += text;
      },
      run: writing("export default async () => new Response('no', { status: 500 });")
    };
    await expect(runFnDev("plain", {}, deps)).rejects.toMatchObject({ code: ExitCode.FAILURE });
    expect(printed).toContain("plain → 500");
    expect(printed).toContain("host ops: none");
  });

  it("lists each call with its outcome", async () => {
    const result = await devOnce(
      "greet",
      { input: '{"name":"Ada"}' },
      { cwd: dir, run: writing(GREET) }
    );
    const report = renderReport(result);
    expect(report).toContain("host ops: email.send");
    expect(report).toMatch(/email\.send {2}ok {2}stubbed {2}\{.*"subject":"Hi Ada"/);
  });

  it("labels a local run as not the isolate", () => {
    expect(LOCAL_RUN_NOTICE).toMatch(/^LOCAL RUN, not the isolate/);
    expect(LOCAL_RUN_NOTICE).toContain("stubbed");
  });
});
