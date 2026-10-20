/**
 * `oxyc` calls `/api/customer-apps` and nothing else for the app registry.
 *
 * The server no longer mounts `/api/admin/apps/*` (it was a short-lived alias of
 * the same handlers). This test keeps `oxyc` from calling it again: a path
 * written back into any source or test file fails here, not against a server
 * that answers 404.
 */

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SRC = dirname(fileURLToPath(import.meta.url));
const SELF = fileURLToPath(import.meta.url);
// Assembled so this file does not contain the path it forbids.
const FORBIDDEN = ["/api", "admin", "apps"].join("/");

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return /\.(ts|tsx|js|mjs|json)$/.test(entry.name) ? [path] : [];
  });
}

describe("the admin apps alias", () => {
  it("is not named anywhere under sdk/cli/src", () => {
    const files = sourceFiles(SRC);
    // A walk that found nothing would pass for the wrong reason.
    expect(files.length).toBeGreaterThan(50);
    const offenders = files
      .filter((file) => file !== SELF)
      .filter((file) => readFileSync(file, "utf8").includes(FORBIDDEN))
      .map((file) => relative(SRC, file));
    expect(offenders, `${FORBIDDEN} is a removable alias; call /api/customer-apps`).toEqual([]);
  });
});
