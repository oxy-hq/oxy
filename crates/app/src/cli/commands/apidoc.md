## Overview

The **Oxy HTTP API** lets you query databases, manage threads, trigger
workflows, and configure workspaces programmatically.  All endpoints live
under `/api/` and are documented in the sections below.

---

## Authentication

Two schemes are accepted on every endpoint — supply one:

| Scheme | How to obtain | Header sent |
|--------|--------------|-------------|
| **API Key** | Generate in *Settings → API Keys* | `X-API-Key: <key>` |
| **Bearer (JWT)** | Issued by `oxy login` or the magic-link flow | `Authorization: Bearer <token>` |

Use the **Authorize** button above to enter either credential; Swagger UI
will attach it to every "Try it out" request.

---

## CLI Quick-start

### `oxy login` — authenticate the CLI

```
oxy login [--env <env>] [--target <url>]
```

Opens your browser to the Oxy web app, captures the session JWT via a
loopback callback, and caches it in `~/.config/oxy/credentials.json` (keyed
by host so dev / prod tokens are stored separately).

```bash
oxy login                        # authenticate against production
oxy login --env local            # authenticate against a local oxy serve
oxy login --target https://my.oxy.example.com
```

After login, every `oxy api` call for that target is automatically
authenticated — no manual token management needed.

```bash
oxy logout                       # clear the cached token for the default env
oxy logout --env local
```

---

### `oxy api` — call the HTTP API from the terminal

```
oxy api <path> [-X METHOD] [-d BODY | -f key=val ...] [-H Header: value] [--env <env>]
oxy api --routes [FILTER] [--json]
oxy api --openapi
```

An authenticated `curl`/`gh api`-style client.  The path is taken relative
to the target's `/api/` surface; a leading `/` or `api/` prefix is
normalised automatically.

The command is self-describing, which matters when you (or an agent) have a
terminal but not this page:

- **`oxy api --help`** — the full usage guide plus every route the binary
  mounts, grouped by the credential each surface expects.
- **`oxy api --routes <filter>`** — matching routes with what the server says
  each one does; `--json` adds the fleet role and path parameters.
- **`oxy api --openapi`** — this very document, offline: the same spec served
  at `/apidoc/openapi.json`, so the schemas are reachable without a server.

The route table is generated from the router at build time, so it covers the
whole surface rather than the curated subset the schemas below describe.

```bash
# GET examples
oxy api user                                    # GET /api/user
oxy api projects/<id>/agents                   # GET /api/projects/<id>/agents
oxy api threads --env local                    # hit a local oxy serve

# POST with JSON fields
oxy api projects/<id>/runs -f workflow_path=my-workflow.yml

# POST with a raw JSON body
oxy api projects/<id>/query -d '{"sql":"select 1"}'

# POST from a file
oxy api projects/<id>/query -d @payload.json

# POST from stdin
echo '{"sql":"select 1"}' | oxy api projects/<id>/query -d -

# Custom method / headers
oxy api projects/<id>/runs/<run_id> -X DELETE
oxy api something -H 'X-My-Header: value'

# Pipe to jq
oxy api threads | jq '.[].id'

# Print the bearer token (useful for raw curl calls)
oxy api --print-token --env local
```

The `--env` flag resolves the base URL from an `oxy-app.json`
`environments` map in the current directory, or from the built-in defaults
(`local` → `http://localhost:5173`, `production` → your cloud workspace).
Pass `--target <url>` to override it explicitly.

---

## Base URL

All paths in this document are relative to `/api`.  When using `oxy api` the
prefix is added automatically.
