# `@oxy-hq/create-oxy-app`

Scaffold a new Oxy custom app from a curated template:

```bash
pnpm create @oxy-hq/oxy-app my-app            # frontend-only (default: vite)
pnpm create @oxy-hq/oxy-app my-app -t functions   # + Oxy Functions and emails
```

Templates: `vite`, `functions`, `dashboard`, `single-store`.

## Two things to know before changing `templates/`

**1. Every template dir ships down two paths, and they are not equivalent.**

- This CLI scaffolds a **standalone** app that owns its whole directory.
- `crates/app/src/custom_app_template/` bakes the same dirs in with
  `include_dir!` and scaffolds into `apps/<org>/<app>/` of the customer-apps
  **monorepo**, which already owns things at its root.

Anything that would collide with what the monorepo owns at root must carry a
`.yml.example` / `.yaml.example` suffix. The Rust side filters both spellings;
`templateDestName` in `src/cli.ts` renames both back for the standalone case.
The two lists must stay in agreement — a file the server filters but the CLI
does not rename simply vanishes from standalone scaffolds.

Today that covers the shared deploy workflow and, in the `functions` template,
`pnpm-workspace.yaml`: pnpm resolves a workspace root as the nearest ancestor
`pnpm-workspace.yaml`, so a real one at `apps/<org>/<app>/` would make each app
its own root and cut it off from the monorepo's overrides, catalog, and
`workspace:` links to the shared packages bundles import.

**2. Publish `@oxy-hq/vite-plugin` before `@oxy-hq/create-oxy-app`.**

The templates pin the plugin by version range, so a `create-oxy-app` release
that references a plugin version not yet on npm makes every scaffold fail at
`pnpm install`. The *SDK Publish* workflow offers each package individually, so
the `all` path happening to be correctly ordered is not protection.
