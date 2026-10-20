/**
 * `oxyc fn dev --watch`: a save re-runs the handler, and `--input -` is read
 * from stdin ONCE per command. Stdin is stubbed to what a pipe does: its bytes
 * the first time, EOF (an empty string) after. Waits are on conditions, never
 * on durations, because `fs.watch` latency depends on the machine's load.
 */

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Runner } from "../publish/functions.js";

let stdinReads = 0;
vi.mock("../util/data-input.js", () => ({
  resolveDataInput: (value: string | undefined) => {
    if (value === "-") return stdinReads++ === 0 ? '{"name":"Ada"}' : "";
    return value ?? "{}";
  }
}));

const { runFnDev } = await import("./fn-dev.js");

let dir: string;
beforeEach(() => {
  stdinReads = 0;
  dir = mkdtempSync(join(tmpdir(), "oxyc-fn-dev-watch-"));
  writeFileSync(
    join(dir, "oxy-app.json"),
    JSON.stringify({ schemaVersion: 2, slug: "w", name: "W", functions: { h: { route: true } } })
  );
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** An esbuild stand-in: the handler answers with the body it was given. */
const echo: Runner = (_label, _command, args) => {
  const outfile = args.find((a) => a.startsWith("--outfile="))?.slice("--outfile=".length);
  writeFileSync(outfile as string, "export default async (req) => new Response(req.body);");
};

async function until(condition: () => boolean, what: string): Promise<void> {
  const deadline = Date.now() + 30_000;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

describe("fn dev --watch", () => {
  it("re-runs on a save and keeps the piped body", async () => {
    let printed = "";
    const stopped = runFnDev(
      "h",
      { input: "-", watch: true },
      {
        cwd: dir,
        run: echo,
        stdout: (text) => {
          printed += text;
        }
      }
    );
    await until(() => printed.includes('{"name":"Ada"}'), "the first run");

    // Rewrite until a second run lands: the watcher may not be armed the instant
    // the first run prints, and a write made before it is armed is missed. Slower
    // than the debounce, or the debounce would never settle.
    let n = 0;
    let wroteAt = 0;
    await until(() => {
      if (Date.now() - wroteAt > 600) {
        wroteAt = Date.now();
        writeFileSync(join(dir, "oxy-app.json"), readManifest(n++));
      }
      return printed.split("h →").length > 2;
    }, "the re-run");

    expect(printed.split('{"name":"Ada"}').length).toBeGreaterThan(2);
    expect(stdinReads).toBe(1);
    process.emit("SIGINT");
    await stopped;
  }, 60_000);
});

/** The manifest with a changing `name`, so every write is a real change. */
function readManifest(n: number): string {
  return JSON.stringify({
    schemaVersion: 2,
    slug: "w",
    name: `W${n}`,
    functions: { h: { route: true } }
  });
}
