#!/usr/bin/env node
/**
 * Compile `oxyc` to standalone executables — one file, no Node, no npm.
 *
 * `bun build --compile` bundles the JavaScript and a Bun runtime into a single
 * executable. Combined with `embed-assets.mjs`, which puts `template/`,
 * `skills/` and `json-schemas/` inside that bundle, the result is genuinely
 * self-contained: the curl installer drops one file into `~/.local/bin` and
 * every command works, including the ones that need those directories.
 *
 * Output names use the Rust target triples rather than Bun's own
 * (`bun-darwin-arm64`), so `install_oxyc.sh` can reuse `install_oxy.sh`'s
 * uname-to-triple mapping verbatim and the two installers stay comparable.
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const pkgRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(pkgRoot, "binaries");

/** Rust triple (what we publish as) → Bun target (what we build with). */
const TARGETS = {
  "aarch64-apple-darwin": "bun-darwin-arm64",
  "x86_64-apple-darwin": "bun-darwin-x64",
  "aarch64-unknown-linux-gnu": "bun-linux-arm64",
  "x86_64-unknown-linux-gnu": "bun-linux-x64"
};

const only = process.argv.slice(2).filter((a) => !a.startsWith("-"));
const selected = only.length > 0 ? only : Object.keys(TARGETS);
for (const triple of selected) {
  if (!(triple in TARGETS)) {
    console.error(`unknown target ${triple}; known: ${Object.keys(TARGETS).join(", ")}`);
    process.exit(2);
  }
}

// Bun is not a dependency of this package: it is needed to CUT A RELEASE, not
// to develop against one, and adding it to `devDependencies` would make every
// `pnpm install` in the monorepo download a ~90 MB runtime nobody's dev loop
// uses. CI installs it pinned; this is the message for everyone else.
try {
  execFileSync("bun", ["--version"], { stdio: "ignore" });
} catch {
  console.error(
    "bun is required to compile standalone binaries, and was not found on PATH.\n" +
      "  install: curl -fsSL https://bun.sh/install | bash\n" +
      "  (only needed for `build:binary` — `pnpm build` and the tests do not use it)"
  );
  process.exit(1);
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

for (const triple of selected) {
  const outfile = join(outDir, `oxyc-${triple}`);
  process.stdout.write(`building oxyc-${triple} ... `);
  execFileSync(
    "bun",
    [
      "build",
      "--compile",
      `--target=${TARGETS[triple]}`,
      // Strip debug symbols and minify: these ship over the network on every
      // `curl | bash`, and the bundle is the part we control.
      "--minify",
      "--sourcemap=none",
      join(pkgRoot, "src", "main.ts"),
      "--outfile",
      outfile
    ],
    { cwd: pkgRoot, stdio: ["ignore", "pipe", "inherit"] }
  );
  console.log(`${(statSync(outfile).size / 1024 / 1024).toFixed(1)} MB`);
}

console.log(`\n${selected.length} binaries in ${outDir}`);
