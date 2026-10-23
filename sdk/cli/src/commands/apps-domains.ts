/**
 * `oxyc apps domains [list] | add | verify | remove` — serve a custom app on a
 * domain the customer owns (`app.acme.com`) instead of the Oxy subdomain.
 *
 *   GET    /api/customer-apps/{id}/domains                  → {domains, cname_target}
 *   POST   /api/customer-apps/{id}/domains                  → 201, one domain (pending)
 *   POST   /api/customer-apps/{id}/domains/{domainId}/verify → 200, one domain
 *   DELETE /api/customer-apps/{id}/domains/{domainId}       → 204
 *
 * `list` is read-only; `add`, `verify` and `remove` write, so they are the one
 * part of `oxyc apps` that is not a GET. `<app>` resolves as every other
 * `apps` verb does (`openApp`): `<org>/<app>` or an app UUID.
 *
 * A DOMAIN IS NAMED, NOT NUMBERED. The routes take a row id, but an operator
 * types the domain, so `verify` and `remove` read the list and find the row by
 * name (case-insensitive, a trailing dot ignored). A name no row carries is
 * NOT_FOUND, never a guess.
 *
 * EXIT CODES. A rejected request maps by status like the rest of the CLI
 * (400/409 → REQUEST, 404 → NOT_FOUND), with the server's own message and its
 * `error` code (`invalid_domain`, `domain_exists`, `domain_taken`) carried on
 * the error. `verify` that comes back still `pending` is NOT a success: DNS
 * has not propagated or does not match, so it exits FAILURE after printing the
 * checker's reason — a script that runs `verify` in a retry loop branches on it.
 */

import { type ApiResponse, errorForResponse, parseJson, request } from "../api/request.js";
import type { Context } from "../context/resolve.js";
import * as log from "../ui/log.js";
import { heading, table } from "../ui/render.js";
import { CliError, ExitCode, usageError } from "../util/errors.js";
import { printJson, when } from "./apps.js";
import {
  type AppRow,
  appLabel,
  type Conn,
  CUSTOMER_APPS,
  getJson,
  openApp
} from "./apps-client.js";

/** One custom domain of an app — the server's `DomainView`. */
export interface DomainView {
  id: string;
  domain: string;
  status: "pending" | "verified";
  url: string;
  /** The DNS name the ownership TXT record goes on. */
  txt_name: string;
  txt_value: string;
  verified_at: string | null;
  last_checked_at: string | null;
  last_check_error: string | null;
  created_at: string;
}

interface DomainList {
  domains: DomainView[];
  /** Where the customer's CNAME points. `null`: this deployment has not configured one. */
  cname_target: string | null;
}

// ── wire ───────────────────────────────────────────────────────────────────

function domainsPath(app: AppRow, ...rest: string[]): string {
  return [`${CUSTOMER_APPS}/${app.id}/domains`, ...rest.map(encodeURIComponent)].join("/");
}

/**
 * The error for a non-2xx answer on these routes.
 *
 * They answer `{error, message}`, where the generic path reads `{code, message}`;
 * so the message is picked out here and the `error` string rides along as the
 * server code. The exit code is still the status's.
 */
function domainError(response: ApiResponse): CliError {
  const generic = errorForResponse(response);
  const body = parseJson(response.body) as { error?: unknown; message?: unknown } | undefined;
  if (typeof body?.message !== "string" || generic.code === ExitCode.AUTH) return generic;
  return new CliError(body.message, {
    code: generic.code,
    hint: generic.hint,
    serverCode: typeof body.error === "string" ? body.error : undefined,
    serverMessage: body.message
  });
}

async function send(
  conn: Conn,
  method: "GET" | "POST" | "DELETE",
  path: string,
  body?: unknown
): Promise<ApiResponse> {
  const response = await request({
    target: conn.target,
    path,
    method,
    body: body === undefined ? undefined : JSON.stringify(body),
    bearer: conn.bearer,
    headers: conn.headers
  });
  if (response.status < 200 || response.status >= 300) throw domainError(response);
  return response;
}

function asDomain(response: ApiResponse, what: string): DomainView {
  const parsed = parseJson(response.body) as Partial<DomainView> | undefined;
  if (!parsed?.id || typeof parsed.domain !== "string") {
    throw new CliError(`${what} did not return a domain`, { code: ExitCode.FAILURE });
  }
  return parsed as DomainView;
}

async function fetchDomains(conn: Conn, app: AppRow): Promise<DomainList> {
  const path = domainsPath(app);
  const list = await getJson<Partial<DomainList>>(conn, path);
  if (!Array.isArray(list.domains)) {
    throw new CliError(`GET ${path} did not return a domain list`, { code: ExitCode.FAILURE });
  }
  return { domains: list.domains, cname_target: list.cname_target ?? null };
}

/** `Acme.COM.` → `acme.com`. */
export function normalizeDomain(domain: string): string {
  return domain.trim().toLowerCase().replace(/\.$/, "");
}

/** The row named `domain`, or a NOT_FOUND that says what the app does have. */
function findDomain(list: DomainList, app: AppRow, domain: string): DomainView {
  const wanted = normalizeDomain(domain);
  const row = list.domains.find((d) => normalizeDomain(d.domain) === wanted);
  if (row) return row;
  const have = list.domains.map((d) => d.domain).join(", ");
  throw new CliError(`${appLabel(app)} has no domain "${domain}"`, {
    code: ExitCode.NOT_FOUND,
    hint: have
      ? `its domains: ${have}`
      : `add one: oxyc apps domains add ${appLabel(app)} ${domain}`
  });
}

// ── output ─────────────────────────────────────────────────────────────────

/** The `oxyc apps domains <verb> …` text for the next step, repeating a non-default `--env`. */
function command(ctx: Context, verb: string, app: AppRow, domain: string): string {
  const env = ctx.flags.env && ctx.flags.env !== "production" ? ` --env ${ctx.flags.env}` : "";
  return `oxyc apps domains ${verb} ${appLabel(app)} ${domain}${env}`;
}

/** The two records a customer creates at their DNS provider. */
function dnsRecords(d: DomainView, cnameTarget: string | null): string[] {
  const cname = cnameTarget ?? "(this deployment has no CNAME target configured — ask Oxy ops)";
  return [`  CNAME  ${d.domain}  →  ${cname}`, `  TXT    ${d.txt_name}  =  ${d.txt_value}`];
}

function printDnsInstructions(
  ctx: Context,
  app: AppRow,
  d: DomainView,
  cnameTarget: string | null
): void {
  const lines = [
    `DNS records to create for ${d.domain}:`,
    ...dnsRecords(d, cnameTarget),
    "",
    `Once they resolve, run: ${command(ctx, "verify", app, d.domain)}`
  ];
  process.stdout.write(`${lines.join("\n")}\n`);
}

// ── commands ───────────────────────────────────────────────────────────────

export async function runAppsDomainsList(ctx: Context, app: string, json: boolean): Promise<void> {
  const { conn, row } = await openApp(ctx, app);
  const list = await fetchDomains(conn, row);

  if (json) {
    printJson({ app: appLabel(row), app_id: row.id, ...list });
    return;
  }
  if (list.domains.length === 0) {
    log.info(`${appLabel(row)} has no custom domains`);
    process.stdout.write(`add one: oxyc apps domains add ${appLabel(row)} <domain>\n`);
    return;
  }

  const lines = [
    `${appLabel(row)} — ${list.domains.length} custom domain${list.domains.length === 1 ? "" : "s"}`,
    table(list.domains, [
      { header: "DOMAIN", value: (d) => d.domain },
      { header: "STATUS", value: (d) => d.status },
      { header: "URL", value: (d) => d.url },
      { header: "LAST CHECKED", value: (d) => when(d.last_checked_at) }
    ])
  ];
  for (const d of list.domains.filter((x) => x.status === "pending")) {
    lines.push(
      heading(`${d.domain} — pending: create these DNS records`),
      ...dnsRecords(d, list.cname_target)
    );
    if (d.last_check_error) lines.push(`  last check failed: ${d.last_check_error}`);
    lines.push(`  then: ${command(ctx, "verify", row, d.domain)}`);
  }
  process.stdout.write(`${lines.join("\n")}\n`);
}

/** The CNAME target for the instructions; a failed read must not hide the domain just added. */
async function cnameTargetAfterAdd(conn: Conn, app: AppRow): Promise<string | null> {
  try {
    return (await fetchDomains(conn, app)).cname_target;
  } catch (cause) {
    if (!(cause instanceof CliError)) throw cause;
    log.warn(`the domain was added, but its CNAME target could not be read: ${cause.message}`);
    return null;
  }
}

export async function runAppsDomainsAdd(
  ctx: Context,
  app: string,
  domain: string,
  json: boolean
): Promise<void> {
  const name = domain.trim();
  if (!name || /[\s/]/.test(name)) {
    throw usageError(
      `"${domain}" is not a domain name`,
      "give a bare host name such as app.acme.com — no scheme, path or spaces"
    );
  }
  const { conn, row } = await openApp(ctx, app);
  const response = await send(conn, "POST", domainsPath(row), { domain: name });
  const added = asDomain(response, `POST ${domainsPath(row)}`);
  const cnameTarget = await cnameTargetAfterAdd(conn, row);

  if (json) {
    printJson({ app: appLabel(row), app_id: row.id, domain: added, cname_target: cnameTarget });
    return;
  }
  process.stdout.write(`Added ${added.domain} to ${appLabel(row)} (${added.status}).\n\n`);
  printDnsInstructions(ctx, row, added, cnameTarget);
}

export async function runAppsDomainsVerify(
  ctx: Context,
  app: string,
  domain: string,
  json: boolean
): Promise<void> {
  const { conn, row } = await openApp(ctx, app);
  const list = await fetchDomains(conn, row);
  const found = findDomain(list, row, domain);
  const path = domainsPath(row, found.id, "verify");
  const checked = asDomain(await send(conn, "POST", path), `POST ${path}`);

  if (json) {
    printJson({
      app: appLabel(row),
      app_id: row.id,
      domain: checked,
      cname_target: list.cname_target
    });
  } else if (checked.status === "verified") {
    process.stdout.write(`${checked.domain} is verified — ${checked.url}\n`);
  }
  if (checked.status === "verified") return;

  if (!json) {
    process.stdout.write(
      `${[`${checked.domain} is still pending.`, ...dnsRecords(checked, list.cname_target)].join("\n")}\n`
    );
  }
  throw new CliError(`${checked.domain} is not verified yet`, {
    code: ExitCode.FAILURE,
    detail: checked.last_check_error ? `last check: ${checked.last_check_error}` : undefined,
    hint: `DNS changes can take a while to propagate — create the records above, then re-run: ${command(ctx, "verify", row, checked.domain)}`
  });
}

export async function runAppsDomainsRemove(
  ctx: Context,
  app: string,
  domain: string,
  json: boolean
): Promise<void> {
  const { conn, row } = await openApp(ctx, app);
  const found = findDomain(await fetchDomains(conn, row), row, domain);
  await send(conn, "DELETE", domainsPath(row, found.id));

  if (json) {
    printJson({ app: appLabel(row), app_id: row.id, removed: found.domain, id: found.id });
    return;
  }
  process.stdout.write(`Removed ${found.domain} from ${appLabel(row)}.\n`);
  if (found.status === "verified") {
    process.stdout.write(
      `${found.url} no longer serves this app; its DNS records can be deleted.\n`
    );
  }
}
