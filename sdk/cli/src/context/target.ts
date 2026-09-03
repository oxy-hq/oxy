/**
 * Turning `--env` / `--target` into a base URL, and mining the org slug a
 * pasted URL carries.
 *
 * A port of `crates/app/src/cli/commands/env_url.rs` and the target half of
 * `app_manifest.rs`. Kept faithful rather than improved: `oxy` and `oxyc` share
 * a credentials file keyed by HOST, so the two must agree about which host an
 * `--env` names or they will cache tokens under different keys and each report
 * the other's login as missing.
 *
 * The one thing worth restating, because it is not obvious from the signature:
 * both org host schemes canonicalise back to the *deployment's* product host.
 * `poke-house.oxygen-hq.com` and `acme.oxygen-hq.com` are the same target with
 * different org slugs, so you log in once per deployment and not once per
 * customer.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

/** A resolved `--env`: where to send the request, and whose org it named. */
export interface ResolvedEnv {
  /** Base URL. No trailing slash, no path. */
  target: string;
  /** Org slug the host carried, when it carried one. Never invented. */
  orgSlug?: string;
}

/**
 * Org-subdomain zones and the product host that serves each.
 *
 * Mirrors the server's one-zone-per-deployment model (`OXY_ORG_SUBDOMAIN_ZONE`
 * in `oxy_app_core::org_host_dispatch`). The CLI cannot read that env var, so
 * the well-known deployments are listed; an unknown zone falls through to
 * "the URL is its own target", which is the honest answer for a self-hosted or
 * preview deployment rather than a guess.
 */
const ORG_ZONES: ReadonlyArray<readonly [zone: string, product: string]> = [
  ["oxygen-hq.com", "https://app.oxygen-hq.com"],
  ["dev.oxy.tech", "https://aip.dev.oxy.tech"],
  ["staging.oxy.tech", "https://aip.staging.oxy.tech"]
];

/** Host labels that are infrastructure — present, they imply no org. */
const RESERVED_LABELS = new Set([
  "app",
  "aip",
  "www",
  "api",
  "admin",
  "static",
  "assets",
  "cdn",
  "docs",
  "customer-apps",
  "customerapps"
]);

/**
 * Built-in target for a well-known env name.
 *
 * `local` is the Vite dev server on 5173, NOT oxy's own 3000 — and that is
 * load-bearing rather than a preference. `oxyc login` opens `<target>/cli-auth`,
 * a route that exists only in the live web app, while `oxy serve` serves a
 * pre-built embedded bundle that may predate it. Vite proxies `/api/*` through
 * to :3000, so requests work either way; login only works through 5173.
 */
export function defaultTarget(env: string): string | undefined {
  switch (env) {
    case "local":
      return "http://localhost:5173";
    case "dev":
    case "development":
      return "https://aip.dev.oxy.tech";
    case "staging":
      return "https://aip.staging.oxy.tech";
    case "production":
    case "prod":
      return "https://app.oxygen-hq.com";
    default:
      return undefined;
  }
}

/**
 * Should this `--env` value be read as a URL rather than an env name?
 *
 * Env names are bare identifiers; a `:`, `/` or `.` means somebody pasted an
 * address bar. Permissive about the scheme so a copied `app.oxygen-hq.com`
 * works without the `https://`.
 */
export function looksLikeUrl(value: string): boolean {
  const v = value.trim();
  if (!v) return false;
  return v.includes("://") || v.includes(".") || v.includes("/") || v.includes(":");
}

/**
 * Add a scheme so the value parses. Loopback gets `http` — nobody runs TLS on
 * a local `oxy serve` — everything else `https`.
 */
function withScheme(value: string): string {
  if (value.includes("://")) return value;
  // A bracketed IPv6 literal is taken through its closing `]`; splitting on
  // ':' first would chop `[::1]:3000` into `[`.
  const host = value.startsWith("[")
    ? `${value.split("]")[0]}]`
    : (value.split(/[/:]/)[0] ?? value);
  const loopback = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]"].includes(host);
  return `${loopback ? "http" : "https"}://${value}`;
}

/**
 * `scheme://host[:port]`. Path, query and fragment are dropped because the URL
 * a user pastes is a *page* (`/orgs/…/threads/…`), not an API base. `--target`
 * is the escape hatch that stays verbatim, for a deployment under a path.
 */
function baseUrl(url: URL): string {
  const host = url.hostname.replace(/\.+$/, "").toLowerCase();
  return url.port ? `${url.protocol}//${host}:${url.port}` : `${url.protocol}//${host}`;
}

/** The single label of `host` inside `zone`. A multi-label prefix is refused. */
function labelInZone(host: string, zone: string): string | undefined {
  const suffix = `.${zone}`;
  if (!host.endsWith(suffix)) return undefined;
  const label = host.slice(0, -suffix.length);
  if (!label || label.includes(".")) return undefined;
  return label;
}

/** Org slug carried by `<org>--<app>.customer-apps.<zone>`. */
function customAppOrg(host: string, zone: string): string | undefined {
  const label = labelInZone(host, `customer-apps.${zone}`);
  if (!label) return undefined;
  const idx = label.indexOf("--");
  if (idx <= 0) return undefined;
  const org = label.slice(0, idx);
  const app = label.slice(idx + 2);
  if (!org || !app) return undefined;
  return org;
}

/** Resolve a pasted URL to a target, plus the org slug its host named. */
export function parseEnvUrl(value: string): ResolvedEnv | undefined {
  let url: URL;
  try {
    url = new URL(withScheme(value.trim()));
  } catch {
    return undefined;
  }
  const host = url.hostname.replace(/\.+$/, "").toLowerCase();

  for (const [zone, product] of ORG_ZONES) {
    // Custom-app subdomain first: its host also ends in the org zone, but its
    // label carries a `--` pair the org rule would mis-read as a slug.
    const appOrg = customAppOrg(host, zone);
    if (appOrg) return { target: product, orgSlug: appOrg };

    const label = labelInZone(host, zone);
    if (!label) continue;
    if (RESERVED_LABELS.has(label)) return { target: product };
    return { target: product, orgSlug: label };
  }

  return { target: baseUrl(url) };
}

/** The subset of `oxy-app.json` this CLI reads. Unknown fields are ignored. */
export interface OxyAppManifest {
  slug?: string;
  orgSlug?: string;
  name?: string;
  environments?: Record<string, { target?: string }>;
}

/** Load `<dir>/oxy-app.json`, or `undefined` if absent or unparsable. */
export function loadManifest(dir: string): OxyAppManifest | undefined {
  try {
    return JSON.parse(readFileSync(join(dir, "oxy-app.json"), "utf8")) as OxyAppManifest;
  } catch {
    return undefined;
  }
}

/**
 * Resolve the deployment to talk to.
 *
 * Precedence, matching the Rust exactly:
 *   `--target`  →  manifest `environments.<env>.target`  →  built-in default
 *   →  the `--env` value read as a URL
 *
 * The URL reading is last and purely additive: every named env keeps working
 * exactly as before, and a name always beats the URL interpretation. That
 * ordering is why `--env local` never tries to resolve `local` as a hostname.
 */
export function resolveEnv(
  env: string | undefined,
  targetFlag: string | undefined,
  manifest?: OxyAppManifest
): ResolvedEnv | undefined {
  // `--target` is the explicit escape hatch and stays verbatim, including for
  // a deployment served under a path. Its org slug is still mined, so
  // `--target https://<org>.oxygen-hq.com` knows which org it points at.
  if (targetFlag?.trim()) {
    const verbatim = targetFlag.trim().replace(/\/+$/, "");
    return { target: verbatim, orgSlug: parseEnvUrl(verbatim)?.orgSlug };
  }

  const name = env?.trim();
  if (!name) return undefined;

  const fromManifest = manifest?.environments?.[name]?.target?.trim();
  if (fromManifest) {
    return {
      target: fromManifest.replace(/\/+$/, ""),
      orgSlug: parseEnvUrl(fromManifest)?.orgSlug
    };
  }

  const builtin = defaultTarget(name);
  if (builtin) return { target: builtin };

  if (looksLikeUrl(name)) return parseEnvUrl(name);
  return undefined;
}
