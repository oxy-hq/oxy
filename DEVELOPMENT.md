# Development Guide

This guide will help you set up your development environment for contributing to Oxy.

## Prerequisites

- Rust **1.92.0 or newer** (the workspace MSRV; edition 2024)
- Node.js and pnpm (pnpm only — never npm or yarn)
- Git
- [`just`](https://github.com/casey/just) — the command runner the whole dev
  workflow is built around (`cargo install just`, or `brew install just` on macOS)

## Clone the repository

```bash
git clone https://github.com/oxy-hq/oxygen.git
cd oxy
```

## Setup

The paved path is `just`, which bootstraps the dev toolchain (including
[`cargo-nextest`](https://nexte.st/), the test runner used everywhere) and
installs frontend dependencies in one step:

```bash
just install   # installs nextest + other dev tools and runs `pnpm install`
just           # list every available recipe
```

If you'd rather install the pieces by hand:

```bash
cargo build      # build the Rust workspace (debug — never use --release locally)
pnpm install     # install frontend dependencies, and point git at .githooks
```

Either route installs the git hooks: `pnpm install` runs a `prepare` script that
sets `core.hooksPath` to the tracked `.githooks` dir, which forwards commit-msg /
pre-commit / pre-push to `.husky/` and seeds a new checkout's `target/`.
`just install-hooks` does the same thing explicitly.

**That first `cargo build` is ~7 minutes from cold, and you can skip nearly all
of it** if you already have another checkout of this repo built:
`just seed-target` hardlinks its artifacts across in seconds. See
[internal-docs/rust-build-performance.md](internal-docs/rust-build-performance.md).

## Environment Variables

You typically do not need to manually set environment variables for local development. Oxy manages environment configuration inside the app by default.

Set environment variables only when integrating external services or overriding defaults (for example, custom database URLs or provider credentials). Use `.env.example` as the reference template.

## Running Tests

Tests run under [`cargo-nextest`](https://nexte.st/), **not** `cargo test`. The
simplest entry point is:

```bash
just test
```

To run nextest directly (e.g. to scope to a crate or a single test):

```bash
cargo nextest run                 # whole workspace
cargo nextest run -p oxy-app      # a single crate
cargo nextest run <test_name>     # a single test by name
```

nextest shows failing-test output by default; add `--no-capture` to stream
stdout from passing tests too.

## Seed Test Data

The `seed` command is **deprecated**. It was built for an older data model and may not align with current org/project-aware flows.

For development testing, prefer creating test data through normal app workflows.

In development mode, if no authentication headers are provided, the system defaults to `guest@oxy.local`:

```bash
# Start the server
cargo run serve

# Test API - will use Guest by default
curl http://localhost:3000/api/user
curl http://localhost:3000/api/threads
```

## Web server

Start the development server:

```bash
cargo run serve
```

This will only start the api server (or in some cases, with a frontend that is resulted from `pnpm build`)
If you need to start the frontend, you can do so with the following commands:

```bash
pnpm run dev
```

The API server will be available at `http://localhost:3000`.
The frontend will be available at `http://localhost:5173`.

## Running multiple instances side by side

You can run several local Oxy instances at once — typically one per git
checkout or [worktree](https://git-scm.com/docs/git-worktree) — to test
multi-instance behavior or work on two branches without stopping each other.

All ports are configurable from the repo-root `.env`, which **both** the backend
(`cargo run serve`, via `dotenv`) and the Vite dev server (`pnpm run dev`, via
`loadEnv`) load. The flags still win when passed explicitly; otherwise the env
var is used, then the default.

| Variable                 | Used by  | Default                 | Purpose                                             |
| ------------------------ | -------- | ----------------------- | --------------------------------------------------- |
| `OXY_HTTP_PORT`          | backend  | `3000`                  | API server port (same as `serve --port`)            |
| `OXY_HTTP_INTERNAL_PORT` | backend  | `3001`                  | internal port (same as `serve --internal-port`)     |
| `OXY_DEV_PORT`           | frontend | `5173`                  | Vite dev server port                                |
| `OXY_DEV_PROXY_TARGET`   | frontend | `http://localhost:3000` | backend the Vite dev server proxies API requests to |

> ### Kubernetes env-var collision
>
> These vars are named `OXY_HTTP_*` — **not** `OXY_PORT` / `OXY_INTERNAL_PORT`,
> and **not** `OXY_SERVE_*`. Kubernetes injects Docker-link service-discovery
> vars of the form `<SVCNAME>_PORT=tcp://<clusterIP>:<port>` into every pod, for
> every Service in the namespace. So an env var `OXY_<X>_PORT` is silently
> overwritten with `tcp://...` whenever a Service named `oxy-<x>` exists — and
> clap then can't parse `tcp://...` as a port, crashlooping the pod (exit 2,
> `invalid digit found in string`).
>
> - `OXY_PORT` collided with the prod Service `oxy`. It shipped in release
>   **0.5.90** and took prod down on **2026-06-29**. Staging was unaffected only
>   because its Service is named `oxy-staging` (→ `OXY_STAGING_PORT`), so the bug
>   passed pre-prod undetected.
> - `OXY_SERVE_PORT` is **also** unsafe: the split-fleet design
>   (`internal-docs/multi-instance-fleet.md`) adds `serve`/`ide`/`worker` roles,
>   and a Service fronting the serve replicas (plausibly `oxy-serve`) would
>   inject `OXY_SERVE_PORT` — the identical crashloop. `http` is not a fleet
>   role, so `oxy-http` is not a plausible Service name.
>
> Rule of thumb: never name an env var `OXY_PORT` or `OXY_<role>_PORT` where
> `oxy`/`oxy-<role>` could be a Service name. **The durable, name-independent fix
> is `enableServiceLinks: false` on the pod spec** (in the `oxy-app` Helm chart,
> `ghcr.io/oxy-hq/helm-charts`), which disables the injection mechanism entirely;
> the `OXY_HTTP_*` naming is just defense-in-depth for deployments that don't set
> it.

Give each checkout its own `.env` with a non-overlapping set of ports, and point
that checkout's frontend at its own backend. For example:

**Checkout A** — `.env`:

```bash
OXY_HTTP_PORT=3000
OXY_HTTP_INTERNAL_PORT=3001
OXY_DEV_PORT=5173
OXY_DEV_PROXY_TARGET=http://localhost:3000
```

**Checkout B** — `.env`:

```bash
OXY_HTTP_PORT=3100
OXY_HTTP_INTERNAL_PORT=3101
OXY_DEV_PORT=5273
OXY_DEV_PROXY_TARGET=http://localhost:3100
```

In each checkout, run `cargo run serve` and `pnpm run dev` as usual. Checkout A
is then at `http://localhost:5173` (API `3000`) and checkout B at
`http://localhost:5273` (API `3100`), fully isolated.

## OAuth bounce proxy (Google / GitHub sign-in across instances)

OAuth providers validate the `redirect_uri` against a fixed allow-list, so every
dev port would otherwise need its own redirect URI registered with Google and
GitHub. The bounce proxy ([`scripts/oauth-bounce.mjs`](scripts/oauth-bounce.mjs))
solves this: you register **one** redirect URI per provider — the proxy's
origin — and it forwards each callback to the instance that started the flow
(identified by the instance origin appended to the OAuth `state`).

It covers Google sign-in and all three GitHub flows (login, account-connect, and
App-install).

### One-time provider setup

Register the proxy's callback URLs (default port `8429`):

- **Google** OAuth client → `http://localhost:8429/auth/google/callback`
- **GitHub OAuth app** → `http://localhost:8429/github/callback`
- **GitHub App** → add `http://localhost:8429/github/callback` as a callback URL
  (GitHub Apps accept multiple callback URLs)

### Per-instance config

Point every instance at the proxy by adding these to each checkout's `.env`:

```bash
OXY_OAUTH_PROXY_ORIGIN=http://localhost:8429    # frontend: redirect_uri target
OXY_OAUTH_REDIRECT_ORIGIN=http://localhost:8429 # backend: token-exchange + GitHub URL building
```

Both must be the same origin (the registered one). Leaving them unset preserves
the normal per-origin flow exactly.

### Run the proxy

Start it once (it is shared by every instance):

```bash
just oauth-proxy            # or: node scripts/oauth-bounce.mjs
```

The listen port can be overridden with `OXY_OAUTH_PROXY_PORT` (default `8429`);
keep it in sync with the origins above and the registered callback URLs. The
proxy only ever forwards to loopback origins, so it is safe to leave running.

## Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) for details on our code of conduct and the process for submitting pull requests.

## Database

Oxy uses PostgreSQL for data storage.

### Development Environment

For local development, Oxy automatically starts an **embedded PostgreSQL instance**. No manual setup required!

The embedded PostgreSQL data is stored in: `~/.local/share/oxy/postgres_data/`

The location can be changed by setting the `OXY_STATE_DIR` environment variable.

### Production/Custom PostgreSQL

To use an external PostgreSQL database, set the `OXY_DATABASE_URL` environment variable:

```bash
export OXY_DATABASE_URL=postgresql://user:password@localhost:5432/oxy
```

### Running Migrations

Migrations are run automatically on startup. To run manually:

```bash
cargo run --bin migration
```

## HTTPS (Optional)

HTTPS is optional for day-to-day local development. Oxy can run locally without requiring TLS setup.

Use local HTTPS only when you need to test HTTPS-only behavior (for example, HTTP/2-only scenarios).

To enable HTTPS locally (backend and frontend), you need TLS certificates. We recommend using [mkcert](https://github.com/FiloSottile/mkcert):

### Install mkcert

**macOS:**

```sh
brew install mkcert
brew install nss # if you use Firefox
```

**Linux:**
Please check for instruction on [mkcert installation](https://github.com/FiloSottile/mkcert#linux).

Trust certificates from mkcert:

```sh
mkcert -install
```

We don't need to generate a self-signed cert for Oxy, as we already bundle a cert into the project.

If you want to run local development with HTTPS/HTTP2, use:

```bash
cargo run serve -- --http2-only
pnpm run dev
```
