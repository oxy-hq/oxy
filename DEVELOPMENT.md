# Development Guide

This guide will help you set up your development environment for contributing to Oxy.

## Prerequisites

- Rust (latest stable version)
- Node.js and pnpm
- Git

## Clone the repository

```bash
git clone https://github.com/oxy-hq/oxy.git
cd oxy
```

## Setup

1. Install Rust dependencies:

```bash
cargo build
```

2. Install Node.js dependencies:

```bash
pnpm install
```

## Local HTTPS Development: TLS Certificates

### Why HTTPS Is Critical for Development

Oxy uses HTTP/2 for its backend and frontend communication during development. Modern browsers and many HTTP clients only enable HTTP/2 when using HTTPS (TLS). This means that for local development, HTTPS is required to fully test and utilize HTTP/2 features, such as multiplexing and improved performance. Without HTTPS, your development environment will fall back to HTTP/1.1, which does not support these advanced features.

To ensure you are developing and testing with HTTP/2, follow the instructions below to set up local TLS certificates using mkcert.

To enable HTTPS for local development (backend and frontend), you need TLS certificates. We recommend using [mkcert](https://github.com/FiloSottile/mkcert):

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

We dont need to generate a self-signed cert for oxy, as we already bundle a cert into the project

## Environment Variables

Set the following environment variables for full functionality:

- `OPENAI_API_KEY` - Required for AI features
- Configurations for external services (e.g., BigQuery, if used, see examples folder for sample configuration)

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

Use the `seed` command to populate your database with test users:

```bash
# Create test users
cargo run -- seed users # or seed full

# Clear all test data when done
cargo run -- seed clear
```

The seeding system creates these test users:

| Email                     | Name        |
| ------------------------- | ----------- |
| `alice.smith@company.com` | Alice Smith |
| `bob.johnson@company.com` | Bob Johnson |
| `guest@oxy.local`         | Guest User  |

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
cargo run serve -- --http2-only  ## frontend only talks to backend via https
pnpm run dev
```

The API server will be available at `https://localhost:3000`.
The frontend will be available at `https://localhost:5173`.

## Building for Production

To build a release version:

```bash
cargo build --release
```

## Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) for details on our code of conduct and the process for submitting pull requests.

## Database Setup

Oxy uses SQLite by default for development. The database file will be created automatically when you first run the application.
Location is `~/.local/share/oxy/db.sqlite`. The location can be changed by setting `OXY_STATE_DIR` environment variable.

For production deployments, you can configure other database backends through environment variables. The variable is `OXY_DATABASE_URL`.
