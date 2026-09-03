/**
 * Turning what somebody typed into the path the server mounts, and filling in
 * the `{...}` placeholders from the resolved context.
 */

import { usageError } from "../util/errors.js";

/**
 * Top-level mounts that are NOT under `/api`.
 *
 * Needed because the normaliser's job is "prefix `/api/` unless it is already
 * a real path", and without this list every one of these gets a second prefix.
 *
 * THIS FIXES A LIVE BUG in the Rust `oxy api`, which is worth stating because
 * the fix looks like gratuitous divergence otherwise. `normalize_path` there
 * tests only for an `api` first segment, so the API-key surface — documented
 * in its own `--help` as `oxy api /external/api/<workspace_id>/sql/query` —
 * normalises to `/api/external/api/<workspace_id>/sql/query` and 404s. The
 * documented example could never have worked.
 */
const TOP_LEVEL_MOUNTS = new Set([
  "api",
  "external",
  "apidoc",
  "customer-apps",
  "healthz",
  "readyz",
  "livez",
  "metrics"
]);

/**
 * Normalise a user-supplied path to something the server mounts.
 *
 * `user` → `/api/user`; `/user` → `/api/user`; `api/user` → `/api/user`;
 * `/external/api/x/sql/query` → itself. A leading slash is accepted rather
 * than required because both spellings are natural and neither is wrong.
 */
export function normalizePath(path: string): string {
  const trimmed = path.trim().replace(/^\/+/, "");
  if (!trimmed) return "/api";
  const first = trimmed.split("/")[0] ?? "";
  if (TOP_LEVEL_MOUNTS.has(first)) return `/${trimmed}`;
  return `/api/${trimmed}`;
}

/**
 * Whether a path sits on the API-key-only surface.
 *
 * Used to pick the credential: `/external/api/*` expects `X-API-Key`, every
 * other surface a bearer. Deciding from the path means a caller never has to
 * know which header a route wants — which is the entire reason this command
 * exists rather than a `curl` alias.
 */
export function isExternalSurface(path: string): boolean {
  return path.startsWith("/external/api/");
}

/** The values a `{placeholder}` can be filled from. */
export interface PlaceholderValues {
  org?: string;
  workspace?: string;
  project?: string;
  customer?: string;
  me?: string;
}

/**
 * Substitute `{org}`, `{workspace}`, `{project}`, `{customer}` and `{me}`.
 *
 * `gh api` does this with `{owner}` and `{repo}` from the git remote, and it
 * is the single feature that makes the command usable from memory rather than
 * from a scratchpad of ids. Ours resolve from the customer context, which is
 * what the two merged tools share:
 *
 *   oxyc api {workspace}/threads     # inside a customer repo, no ids typed
 *
 * An UNRESOLVED placeholder is an error, never a literal. Sending
 * `/api/{workspace}/threads` to the server produces a 404 about a workspace
 * literally named `{workspace}`, and the caller has to work backwards from
 * that to "the context did not resolve" — so the failure is raised here,
 * where it can say which value was missing and how to supply it.
 */
export function substitutePlaceholders(path: string, values: PlaceholderValues): string {
  return path.replace(/\{([a-z_]+)\}/g, (match, name: string) => {
    if (!SUPPORTED.includes(name as keyof PlaceholderValues)) {
      // A DIFFERENT failure from "did not resolve", and worth its own message:
      // `could not resolve {nonsence}` reads as a supported placeholder that
      // happens to be unset, sending the caller off to look for the flag that
      // would set it. There is no such flag.
      throw usageError(
        `unknown placeholder ${match}`,
        `supported: ${SUPPORTED.map((n) => `{${n}}`).join(" ")}`
      );
    }
    const value = values[name as keyof PlaceholderValues];
    if (value) return encodeURIComponent(value);
    throw usageError(`could not resolve ${match} in the path`, placeholderHint(name));
  });
}

/** The placeholder names this CLI fills in. */
const SUPPORTED: (keyof PlaceholderValues)[] = ["org", "workspace", "project", "customer", "me"];

/** What to do about a supported placeholder that did not resolve. */
function placeholderHint(name: string): string {
  switch (name) {
    case "org":
      return "pass --org <slug>, or run from a customer repo (`oxyc <customer> --here`)";
    case "workspace":
      return "pass --workspace <id>, or run `oxyc api {org}/workspaces` to find one";
    case "project":
      return "pass --project <id>";
    case "customer":
      return "run inside a customer repo, or name one with --customer";
    default:
      return "run `oxyc login` first — {me} is your own account";
  }
}

/** The placeholder names a path uses, for reporting before a request is made. */
export function placeholdersIn(path: string): string[] {
  return [...path.matchAll(/\{([a-z_]+)\}/g)].map((m) => m[1] as string);
}
