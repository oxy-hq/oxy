# {{APP_DISPLAY_NAME}}

Customer-app bundle scaffolded by `pnpm dlx create-oxy-app`. Vite + React +
`@oxy-hq/sdk`.

## Quick start

```bash
npm install
npm run dev     # local dev on :5174, proxies /api → oxy on :3000
```

Then either:

- Open `http://localhost:5174` directly and paste a
  `window.__OXY_APP__ = { orgSlug: "...", slug: "...", ... }` snippet
  in DevTools to simulate oxy's runtime injection.
- Or register this folder as a customer-app in oxy (admin UI), build
  with `OXY_APP_BASE_PATH=/customer-apps/<org>/<slug>/ npm run build`,
  and open the served URL.

## Editing

- **`public/oxy-app.json`** — declares which data products this
  bundle consumes. Each product names a producer (today: `app_task`
  or `parquet_file`); the server resolves them server-side. Edit
  this when you add a product.
- **`src/App.tsx`** — your UI. Replace the scaffold with whatever
  you actually want to render.

## Deploying

```bash
oxy apps deploy . --org <org_slug> --slug {{APP_SLUG}} \
    --workspace <workspace_uuid>
```

The deploy command builds the bundle, uploads `out/` to the configured
target (local folder or S3), and creates/updates the corresponding
customer-app row in oxy. Idempotent — safe to re-run on CI retries.
