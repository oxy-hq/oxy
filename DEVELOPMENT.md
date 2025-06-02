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

Start the frontend development server:

```bash
pnpm run dev
```

The API server will be available at `http://localhost:3000`.
The frontend will be available at `http://localhost:5173`.

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
