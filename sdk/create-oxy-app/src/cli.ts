/**
 * `create-oxy-app` — scaffold a new Oxy custom app.
 *
 * Usage:
 *   pnpm dlx create-oxy-app my-app
 *   pnpm dlx create-oxy-app my-app --template single-store
 *
 * The CLI copies a template directory from `templates/<id>/` next to
 * this script (bundled with the package) into a new directory at
 * `<cwd>/<name>/`, with `{{name}}` and `{{slug}}` substituted in
 * `package.json` and `oxy-app.json`.
 *
 * Templates ship with the kit deps pre-pinned, the vite-plugin
 * pre-wired in `vite.config.ts`, and a working `useQuery` example.
 * The point: the operator types one command and `pnpm install &&
 * pnpm dev` works.
 */

import { promises as fs } from "node:fs";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
// Templates live next to the bundled CLI. In dev (`pnpm dev` against
// the source) `dist/` is one level under the package root; in
// publish (`pnpm dlx`) `dist/` and `templates/` are siblings under
// the published package. Both shapes resolve to the same parent.
const TEMPLATES_DIR = path.resolve(HERE, "..", "templates");

interface ParsedArgs {
  name: string | null;
  template: string;
  help: boolean;
}

function parseArgs(argv: string[]): ParsedArgs {
  const out: ParsedArgs = { name: null, template: "vite", help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") {
      out.help = true;
    } else if (arg === "--template" || arg === "-t") {
      const next = argv[i + 1];
      if (!next) throw new Error("--template requires a value");
      out.template = next;
      i++;
    } else if (arg.startsWith("--template=")) {
      out.template = arg.slice("--template=".length);
    } else if (!arg.startsWith("-") && !out.name) {
      out.name = arg;
    }
  }
  return out;
}

function printHelp(): void {
  console.log(`
create-oxy-app — scaffold a new Oxy custom app.

Usage:
  create-oxy-app <name> [--template <id>]

Options:
  -t, --template <id>   Template to use (default: vite).
                        Available: vite, single-store, dashboard.
  -h, --help            Show this help.

Examples:
  pnpm dlx create-oxy-app store-pulse
  pnpm dlx create-oxy-app sales-dashboard --template dashboard
`);
}

/**
 * Slugify a name into a valid oxy-app slug: lowercase, alphanumeric
 * + dashes, starts with a letter or digit. Matches the regex the
 * vite-plugin enforces — see validateManifest there.
 */
function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");
}

async function copyTemplate(
  templateDir: string,
  targetDir: string,
  vars: { name: string; slug: string }
): Promise<void> {
  await fs.mkdir(targetDir, { recursive: true });
  const entries = await fs.readdir(templateDir, { withFileTypes: true });
  for (const entry of entries) {
    const src = path.join(templateDir, entry.name);
    // Rename `_gitignore` → `.gitignore` so npm doesn't strip
    // dotfiles from the published package. Same trick as create-vite.
    // Also rename `.example` workflow files to the real path the
    // scaffolded app wants — the `.example` suffix exists so the
    // server-side admin-scaffold can filter them (the customer-apps
    // repo has a shared workflow at root and per-app ones would
    // conflict), but a standalone CLI scaffold genuinely wants a
    // ready-to-edit workflow file.
    let destName = entry.name;
    if (destName === "_gitignore") destName = ".gitignore";
    else if (destName.endsWith(".yml.example")) {
      destName = destName.slice(0, -".example".length);
    }
    const dest = path.join(targetDir, destName);
    if (entry.isDirectory()) {
      await copyTemplate(src, dest, vars);
    } else if (entry.name === "template.json" || entry.name === "screenshot.png") {
      // Metadata for the admin gallery only — don't ship to the
      // scaffolded app.
      continue;
    } else {
      const bytes = await fs.readFile(src, "utf-8");
      // Use the same placeholders as the server-side renderer
      // (crates/app/src/custom_app_template/mod.rs) so a single
      // template set serves both code paths.
      const substituted = bytes
        .replace(/\{\{APP_DISPLAY_NAME\}\}/g, vars.name)
        .replace(/\{\{APP_SLUG\}\}/g, vars.slug);
      await fs.writeFile(dest, substituted, "utf-8");
    }
  }
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  if (!args.name) {
    console.error("error: app name is required");
    printHelp();
    process.exit(1);
  }
  const slug = slugify(args.name);
  if (!/^[a-z0-9][a-z0-9-]*$/.test(slug)) {
    console.error(
      `error: derived slug "${slug}" is invalid; pick a name with at least one ASCII letter or digit`
    );
    process.exit(1);
  }

  const templateDir = path.join(TEMPLATES_DIR, args.template);
  if (!existsSync(templateDir)) {
    console.error(`error: unknown template "${args.template}"`);
    const available = await fs.readdir(TEMPLATES_DIR);
    console.error(`available: ${available.join(", ")}`);
    process.exit(1);
  }

  const targetDir = path.resolve(process.cwd(), args.name);
  if (existsSync(targetDir)) {
    const contents = await fs.readdir(targetDir);
    if (contents.length > 0) {
      console.error(`error: ${targetDir} already exists and is non-empty`);
      process.exit(1);
    }
  }

  console.log(`Creating ${args.name} from template "${args.template}"...`);
  await copyTemplate(templateDir, targetDir, { name: args.name, slug });
  console.log(`
Done. Next steps:

  cd ${args.name}
  pnpm install
  pnpm dev          # Vite dev server at http://localhost:5173
  pnpm run screenshot   # capture public/card.png for the HQ launcher card
                        # (then set "art": "card.png" in oxy-app.json)

Then register the app via the admin UI (Customer apps → Add new).
The Link flow will pick up oxy-app.json automatically.
`);
}

// Run main() only when invoked as a CLI, not when imported by tests.
// `import.meta.url` is the module URL; `process.argv[1]` is the
// script entrypoint Node was invoked with. They match when this
// file is the entry; differ when vitest imports it.
const invokedDirectly =
  import.meta.url === `file://${process.argv[1]}` ||
  import.meta.url.endsWith(path.basename(process.argv[1] ?? ""));

if (invokedDirectly) {
  main().catch((err) => {
    console.error(`error: ${err instanceof Error ? err.message : String(err)}`);
    process.exit(1);
  });
}

// Exported for unit tests.
export { parseArgs, slugify };
