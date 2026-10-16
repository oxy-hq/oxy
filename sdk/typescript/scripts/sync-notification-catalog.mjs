#!/usr/bin/env node
// sync-notification-catalog: copy the notification catalogue's checks into the two packages
// that run them before a publish.
//
// `src/custom-app/notification-catalog.ts` is the source. `oxyc` (sdk/cli) and
// `@oxy-hq/vite-plugin` each carry a byte copy, because neither depends on another package
// here and a fourth package for one file is more machinery than it saves. Each copy's package
// has a `notification-catalog-sync.test.ts` that fails until this has been run after an edit.
//
//   node sdk/typescript/scripts/sync-notification-catalog.mjs          write the copies
//   node sdk/typescript/scripts/sync-notification-catalog.mjs --check  exit 1 if one differs
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const sdk = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const source = join(sdk, "typescript", "src", "custom-app", "notification-catalog.ts");
const copies = [
  join(sdk, "cli", "src", "publish", "notification-catalog.ts"),
  join(sdk, "vite-plugin", "src", "notification-catalog.ts")
];

const bytes = readFileSync(source);
const check = process.argv.includes("--check");
let stale = 0;
for (const copy of copies) {
  let current = null;
  try {
    current = readFileSync(copy);
  } catch {
    // Absent: written below, or stale under --check.
  }
  if (current?.equals(bytes)) continue;
  stale += 1;
  if (check) console.error(`stale: ${relative(sdk, copy)}`);
  else writeFileSync(copy, bytes);
}
if (check && stale > 0) {
  console.error("run `node sdk/typescript/scripts/sync-notification-catalog.mjs`");
  process.exit(1);
}
console.log(
  check ? "notification catalogue copies are current" : `synced ${stale} of ${copies.length} copies`
);
