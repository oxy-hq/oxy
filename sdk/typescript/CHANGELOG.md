# Changelog

All notable changes to the Oxy TypeScript SDK will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2026-05-29

Complete rewrite: `@oxy-hq/sdk` is now a **React-only, customer-app-only**
SDK. A bundle wraps its tree in `<OxyAppProvider>` and reads from its linked
oxy project through hooks; identity is resolved from `oxy-app.json` +
`window.__OXY_APP__` (injected by oxy at serve time), and requests are
authenticated by the session cookie (same-origin) or a bearer token
(cross-origin dev).

### Added

- `OxyAppProvider` — resolves app identity and provides it via context.
- Hooks: `useQuery` (inline SQL), `useSemanticQuery` (semantic layer),
  `useAgentRun` (agent chat over SSE), `useProcedureRun` (long-running
  procedures, beta).
- Drop-in components: `<OxyChat>` and `<OxyAnswer>` (markdown + SQL
  artifacts; URL-scheme allowlist guards against `javascript:` injection).
- `OxyApiError` structured error envelope.
- Pairs with `@oxy-hq/vite-plugin` (base path, manifest copy, dev shim) and
  `create-oxy-app` scaffolding.

### Removed (BREAKING)

- The entire v1 stack: `OxyClient` / `OxySDK` / `OxyProvider`, the
  Parquet/DuckDB-WASM reader, and postMessage-based auth. Apps now talk to
  `/api/projects/:id/*` exclusively.
- `listApps` / `getAppData` / `runApp` / `getDisplays` / `getFile` /
  `getFileUrl`.

## [0.1.0] - 2025-01-01

### Added

- Initial release of the Oxy TypeScript SDK
- Core `OxyClient` with methods for app data fetching
- Configuration management with environment variable support
- Parquet file reading with DuckDB-WASM integration
- `ParquetReader` class for SQL queries on Parquet data
- Helper functions for quick Parquet data access
- Full TypeScript type definitions
- Comprehensive examples for Node.js, React, and v0 integration
- Documentation and API reference

### Features

- `listApps()` - List all apps in a project
- `getAppData()` - Fetch app data with caching
- `runApp()` - Run app and get fresh data
- `getDisplays()` - Get display configurations
- `getFile()` - Fetch files from state directory
- `getFileUrl()` - Get direct file URLs
- Parquet reading and SQL querying capabilities
- Support for both CommonJS and ES modules
- Browser and Node.js compatibility