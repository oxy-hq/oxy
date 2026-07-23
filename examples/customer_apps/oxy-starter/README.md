# Oxy Starter — the example custom app

The smallest complete [Custom App](../../../internal-docs/customer-apps.md): one HTML
file, no build step, no dependencies. `oxy seed` deploys it through the real publish path
(`put_build` → `app_builds` → channel pointers), so it exercises the same serve code an
`oxy publish` bundle does — and a fresh clone lands on a launcher with a working app on it.

```sh
cargo run -p oxy-app -- seed
cargo run -p oxy-app -- compile --workspace-path ./examples
OXY_ROLE=ide cargo run -p oxy-app -- serve --enterprise
# → /customer-apps/local/oxy-starter/
```

## The three patterns it shows

1. **Scalar aggregate** — `POST /api/projects/<projectId>/query` for one row of KPIs.
2. **List query** — the same endpoint for a top-5 leaderboard.
3. **Runtime identity** — `window.__OXY_APP__`, spliced in before `</head>` at serve time.
   A bundle never hardcodes its org or project; it reads them and addresses the data plane
   with `projectId`. The session cookie that authorized the bundle also authorizes the
   query, so there's no second auth step.

Both queries hit `oxymart` in the `local` DuckDB database from `examples/config.yml` — the
same demo table the example dashboards use. No credentials, so it works offline.

## Why it's hand-written

A real app uses `create-oxy-app` (`sdk/create-oxy-app/templates/`) and ships a Vite bundle.
This one stays a single file on purpose: the seed must work on a fresh clone with no Node
toolchain, and an example you can read top-to-bottom in one sitting teaches the contract
better than a build pipeline does.

For the full worked example — semantic queries, Jinja params, Oxy Functions, scheduled
jobs — see `examples/hello-oxy` in the [customer-apps](https://github.com/oxy-hq/customer-apps)
repo. This is the floor; that's the ceiling.

## Editing it

Edit and re-run `oxy seed`. The build id is a content hash, so changed bytes become a new
build and the bundle cache can't serve a stale page. `cargo nextest run -p oxy-app seed_apps`
holds the bundle to the contracts the serve path enforces.
