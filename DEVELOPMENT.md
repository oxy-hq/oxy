# Development Guide

This guide will help you set up your development environment for contributing to Oxy.

## Prerequisites

- Rust (latest stable version)
- Node.js and pnpm
- Git

## Clone the repository

```bash
git clone https://github.com/oxy-hq/oxygen.git
cd oxy
```

## Setup

1. Install Rust dependencies:

```bash
cargo build
```

1. Install Node.js dependencies:

```bash
pnpm install
```

## Environment Variables

You typically do not need to manually set environment variables for local development. Oxy manages environment configuration inside the app by default.

Set environment variables only when integrating external services or overriding defaults (for example, custom database URLs or provider credentials). Use `.env.example` as the reference template.

## Running Tests

To run the test suite:

```bash
cargo test
```

To show test output for debugging:

```bash
cargo test -- --nocapture
```

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
