// Dev/watch runner for the SDK.
//
// tsdown's watcher only tracks its JS/TS module graph, so a standalone edit
// to src/shell/shell.css (which is a copied asset, not a bundled import)
// never triggers a rebuild — dist/shell.css would go stale during `dev`.
// This runner starts `tsdown --watch` AND watches shell.css directly,
// re-copying it on change so live CSS edits are reflected immediately.
//
// The plain `pnpm build` (and every tsdown rebuild) still copies the CSS via
// `onSuccess` in tsdown.config.ts; this script only adds the CSS-only-edit
// case that tsdown can't see. Copies here are idempotent with that.
import { spawn } from "node:child_process";
import { copyFileSync, mkdirSync, watch } from "node:fs";
import { dirname } from "node:path";

const SRC = "src/shell/shell.css";
const OUT = "dist/shell.css";

function copyCss() {
  try {
    mkdirSync(dirname(OUT), { recursive: true });
    copyFileSync(SRC, OUT);
    console.log(`[shell-css] ${SRC} -> ${OUT}`);
  } catch (err) {
    console.error(`[shell-css] copy failed: ${err.message}`);
  }
}

// Watch the directory (not the file) so the watch survives editors that save
// via atomic write-and-rename, which would orphan a file-level watch.
watch(dirname(SRC), { persistent: true }, (_event, filename) => {
  if (filename === "shell.css") copyCss();
});

// tsdown owns the JS/TS build + its own onSuccess CSS copy; inherit its stdio
// so the usual watch output shows through.
const child = spawn("tsdown", ["--watch"], { stdio: "inherit", shell: true });

function shutdown(signal) {
  if (child.exitCode === null && !child.killed) child.kill(signal);
}
process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));
child.on("exit", (code) => process.exit(code ?? 0));
