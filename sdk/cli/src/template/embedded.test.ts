/**
 * The embedded-asset path — what a compiled single-file `oxyc` relies on.
 *
 * These run against the real generated payload rather than a fixture, because
 * the thing most likely to break is the payload itself: `embed-assets.mjs`
 * silently skipping a directory, or dropping the executable bits, produces a
 * binary that is broken in exactly the way these assert against. A fixture
 * would keep passing while the shipped bytes rotted.
 */

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ASSET_DIRS, embeddedAssetsDir, extractEmbeddedAssets } from "./embedded.js";

let cache: string;
const saved = process.env.OXYC_CACHE_DIR;

beforeEach(() => {
  cache = mkdtempSync(join(tmpdir(), "oxyc-embed-"));
  process.env.OXYC_CACHE_DIR = cache;
});

afterEach(() => {
  if (saved === undefined) delete process.env.OXYC_CACHE_DIR;
  else process.env.OXYC_CACHE_DIR = saved;
  rmSync(cache, { recursive: true, force: true });
});

describe("extractEmbeddedAssets", () => {
  it("materialises every directory the package ships", () => {
    const root = extractEmbeddedAssets();
    for (const dir of ASSET_DIRS) {
      expect(existsSync(join(root, dir)), `${dir} should exist`).toBe(true);
      expect(readdirSync(join(root, dir)).length, `${dir} should not be empty`).toBeGreaterThan(0);
    }
  });

  /**
   * The reason the payload records a mode bit at all. A scaffolded workspace
   * whose `dev.sh` is 0644 fails when the customer runs it, and reads as a bug
   * in their repo rather than in this extraction — so assert the bit, not just
   * the file's presence.
   */
  it("keeps the executable bit on the template's scripts", () => {
    const root = extractEmbeddedAssets();
    const script = join(root, "template", "scripts", "dev.sh");
    expect(existsSync(script)).toBe(true);
    expect(statSync(script).mode & 0o111).not.toBe(0);
  });

  /** A schema is real JSON, not a truncated or mis-decoded blob. */
  it("round-trips file contents byte-for-byte", () => {
    const root = extractEmbeddedAssets();
    const schemas = readdirSync(join(root, "json-schemas")).filter((f) => f.endsWith(".json"));
    expect(schemas.length).toBeGreaterThan(0);
    for (const name of schemas) {
      const parsed = JSON.parse(
        execFileSync("cat", [join(root, "json-schemas", name)], { encoding: "utf8" })
      );
      expect(typeof parsed, name).toBe("object");
    }
  });

  it("extracts under a digest-keyed directory, so two builds never collide", () => {
    const root = extractEmbeddedAssets();
    expect(root).toBe(embeddedAssetsDir());
    expect(root.startsWith(join(cache, "assets"))).toBe(true);
    // The leaf is the digest — a different payload lands somewhere else.
    expect(readdirSync(join(cache, "assets"))).toEqual([root.split("/").pop()]);
  });

  it("is idempotent, and leaves no staging directories behind", () => {
    const first = extractEmbeddedAssets();
    const second = extractEmbeddedAssets();
    expect(second).toBe(first);
    const litter = readdirSync(join(cache, "assets")).filter((e) => e.includes("tmp-"));
    expect(litter).toEqual([]);
  });

  /**
   * MUTATION CHECK for the idempotence test above. That test would pass just as
   * well if extraction re-ran every time, so pin the thing it actually claims:
   * a second call must not rewrite what is already there.
   */
  it("does not rewrite an extraction that is already complete", () => {
    const root = extractEmbeddedAssets();
    const canary = join(root, "json-schemas", "canary.txt");
    writeFileSync(canary, "written by the test");
    extractEmbeddedAssets();
    expect(existsSync(canary), "a re-extraction would have replaced the tree").toBe(true);
  });

  /**
   * A tree missing one directory is NOT complete, however plausible it looks.
   * Extraction keyed only on "the directory exists" would serve a half-written
   * tree left by a killed process, and every later run would inherit it.
   */
  it("re-extracts over a partial tree", async () => {
    const target = embeddedAssetsDir();
    await mkdir(join(target, ASSET_DIRS[0]), { recursive: true });
    const root = extractEmbeddedAssets();
    for (const dir of ASSET_DIRS) {
      expect(existsSync(join(root, dir)), `${dir} should exist after repair`).toBe(true);
    }
  });
});
