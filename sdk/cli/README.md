# `@oxy-hq/cli` — `oxyc`

The Oxy CLI. Two halves under one command:

- **Talk to the API** — a `gh api`-shaped client, with discovery, so a caller
  with a token and nothing else can find and call any endpoint.
- **Work on a customer account** — which customer, where their repo is, keeping
  it in line with the template, and scoping a Claude session to them.

They are one tool because they share one answer to "who is this about": stand
in a customer's repo and `oxyc api {org}/workspaces` needs no ids typed.

> **Not on npm yet.** `@oxy-hq/cli` is deliberately absent from the publish
> workflow until npm Trusted Publishing is configured for it, so `npx
> @oxy-hq/cli` and `pnpm add -g @oxy-hq/cli` 404 today. Install from a
> checkout:

```bash
cd sdk/cli && pnpm install && pnpm build
node dist/main.mjs --help        # or `pnpm link --global`, then: oxyc
```

Once it is published, the two lines above become:

```bash
npx @oxy-hq/cli --help          # no install
pnpm add -g @oxy-hq/cli         # then: oxyc
```

## Talking to the API

```bash
oxyc login --env dev            # browser flow; shares its token with `oxy login`
oxyc whoami                     # who the token is, checked against the server

oxyc routes threads             # what exists, and what each endpoint does
oxyc schema {workspace}/threads -X POST    # the body it expects
oxyc api {workspace}/threads --jq '.threads[].title'
```

## Development

```bash
oxyc validate                   # check the workspace's YAML against the schemas
oxyc proxy --env dev            # local app dev against cloud data
oxyc mcp                        # serve the API as MCP tools, for an agent runtime
oxyc guide                      # a page to paste into AGENTS.md / CLAUDE.md
```

**`oxyc validate`** checks `config.yml`, `.automation.yml`, `.agentic.yml`,
`.app.yml` and `.agent.test.yml` against `json-schemas/*.json` — which are
generated from the Rust config types, so this is a second *reader* of one
definition rather than a second definition. It does not replace `oxy validate`:
that one loads the workspace and can resolve `databases:` and `llm.ref`. Where
they disagree, it wins. No network, no token — the one command that works
entirely offline.

**`oxyc proxy`** forwards a dev server's Oxy calls to a cloud target with your
login token attached. Side-effecting calls are **held** and tracking events
**dropped** by default (`--allow-writes` / `--allow-events`); a production
target is refused without `--yes`.

**`oxyc mcp`** serves the API as four MCP tools — `oxy_routes`, `oxy_schema`,
`oxy_request`, `oxy_whoami` — over stdio. Four rather than one per endpoint
because an agent runtime ships every tool's schema on every turn, and ~670 of
them is tens of KB per request; discovery stays a question the agent asks.

```bash
claude mcp add oxyc -- npx -y @oxy-hq/cli mcp --env production
```

`oxyc api` follows `gh api` down to the flag letters:

| | |
| --- | --- |
| `-X, --method` | HTTP method. Defaults to GET, or POST with a body. |
| `-f, --raw-field k=v` | a **string** parameter |
| `-F, --field k=v` | a **typed** parameter — `true` / `3` / `["a"]` / `@file` / `@-` |
| `-F 'ids[]=a' -F 'ids[]=b'` | repeats accumulate into an array |
| `--input @body.json` | raw body from a file, or `-` for stdin |
| `-q, --jq <expr>` | filter through `jq` |
| `--md` | render results as a markdown table — handles arrays of objects, `{columns, rows}`, and the header-row-first arrays `/sql/query` returns |
| `--paginate` / `--slurp` | walk every page; `--slurp` keeps them separate |
| `--cache 5m` | reuse a recent successful GET |
| `-i` / `--silent` / `--verbose` | headers / nothing / log the request |

On GET, HEAD and DELETE, fields become query parameters instead of a body.

**Placeholders.** `{org}`, `{workspace}`, `{project}`, `{customer}` and `{me}`
are filled from context — the customer repo you are standing in, a pasted
`--env` URL, or `--org` / `--workspace` / `--project`. An unresolved
placeholder is an error naming the flag that would fill it, never a literal
sent to the server.

**Surfaces.** `/api/**` takes the bearer from `oxyc login`; `/external/api/**`
takes `X-API-Key` from `$OXY_API_KEY`. The path picks, so you never have to.

**Exit codes** are the point of the tool for a script or an agent — `oxyc
exit-codes` prints them. `4` is "log in again", `5` is "no such thing", `7` is
"retryable", `2` is "you called it wrong".

## Working on a customer account

```bash
oxyc list                       # who the customers are
oxyc <customer>                 # a Claude session scoped to them
oxyc <customer> --here          # …while you work in one of OUR repos
oxyc doctor <customer>          # what the tool knows, changing nothing
oxyc update <customer>          # drift from the workspace template
oxyc adopt <customer>           # give an IMPORTED repo the managed files
oxyc activity <customer>        # merged PRs, theirs and ours
```

## Acting as an org

```bash
oxyc assume start --org acme -r "triage #123"   # 60 min, not renewable
oxyc assume status                              # what is live, minutes left
oxyc assume end                                 # or --all
oxyc login --login-env dev,staging              # log into several at once
```

A staff 403 on a tenant surface usually means **no active session**, not a
mis-modeled role. The session belongs to your account rather than this
terminal, so your browser is in there too — and `end` gets you out of both.

A customer **is** a GitHub repo carrying the `oxy-customer` topic. Tagging the
repo is the whole of registration — there is no customer file anywhere, which
is why there is nothing to keep in sync.

`update` and `adopt` **report by default and write only with `--apply`**, and
neither ever commits, branches or pushes. What each may rewrite is decided by
`template/.oxyc-managed` and nothing else: a file nobody classified is the
customer's and is never touched.

## Environment

| | |
| --- | --- |
| `OXY_TOKEN` | bearer, overriding the login cache (the CI path) |
| `OXY_API_KEY` | key for the `/external/api` surface |
| `OXYC_ORG` | the GitHub org customer repos live in (default `oxy-hq`) |
| `OXYC_DOSSIER_ROOT` | where customer clones go (default `~/.oxyc/dossiers`) |
| `OXYC_TEMPLATE_DIR` | use a working copy of the template instead of the shipped one |
| `NO_COLOR` | plain output |

## Two audiences, one tool

Attached to a terminal you get colour, aligned tables and a searchable picker.
Piped — or read by an agent — you get markdown tables and no colour, because
that is what an LLM reads most reliably and most cheaply. Raw `api` response
bodies are verbatim in both, so `| jq` always works.

Everything that is not the answer goes to **stderr**. stdout carries the
response body and nothing else.

## Requirements

`node >= 20`. `gh` (authenticated) for anything touching the customer registry;
`jq` for `--jq`; `git` for the repo commands. Each is reported by name, with
its install line, the first time something needs it.

Maintainer's notes: [`internal-docs/oxy-api-cli.md`](../../internal-docs/oxy-api-cli.md).
