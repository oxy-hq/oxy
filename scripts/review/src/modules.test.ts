import assert from "node:assert/strict";
import { readdirSync } from "node:fs";
import { test } from "node:test";

// A module that throws while loading (a stray statement left by an edit) breaks the
// reviewer before it can report anything. Every module must at least import.
test("every module loads", async () => {
  const dir = new URL("./", import.meta.url);
  const modules = readdirSync(dir).filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts"));
  assert.ok(modules.length > 10);
  for (const f of modules) await import(new URL(f, dir).href);
});
