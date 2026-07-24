#!/usr/bin/env node
// Local email preview (`pnpm email:dev`): render emails/Welcome.tsx with the
// template's PREVIEW_PROPS sample data and open it in your browser.
//
// It uses preact-render-to-string — the SAME renderer the Oxy Function uses via
// @oxy-hq/sdk/email — so the preview is byte-for-byte what recipients receive.
// No oxy server and no SES involved.

import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { build } from "esbuild";

const root = fileURLToPath(new URL("..", import.meta.url));
const outdir = mkdtempSync(join(tmpdir(), "oxy-email-preview-"));
const bundle = join(outdir, "entry.mjs");

// Bundle a tiny entry that renders the template with its sample props, using
// preact's automatic runtime (same as emails/tsconfig.json).
await build({
  stdin: {
    contents: `
      import { render } from "preact-render-to-string";
      import { h } from "preact";
      import { Welcome, PREVIEW_PROPS } from "./emails/Welcome";
      export const html = render(h(Welcome, PREVIEW_PROPS));
    `,
    resolveDir: root,
    loader: "ts",
    sourcefile: "preview-entry.ts"
  },
  bundle: true,
  format: "esm",
  platform: "node",
  jsx: "automatic",
  jsxImportSource: "preact",
  outfile: bundle
});

const { html } = await import(pathToFileURL(bundle).href);
const out = join(outdir, "welcome.html");
writeFileSync(out, html);
console.log(`Rendered preview → ${out}`);

const opener =
  process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";
spawn(opener, [out], {
  stdio: "ignore",
  detached: true,
  shell: process.platform === "win32"
}).unref();
