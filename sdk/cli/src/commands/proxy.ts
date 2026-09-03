/**
 * `oxyc proxy` — local custom-app dev against a cloud Oxy.
 *
 * An OUTBOUND sidecar: it does not serve the app (your dev server does). It
 * listens on `--port` (default 3000, where a local `oxy serve` would be, so a
 * dev server's existing Oxy proxy target already matches) and forwards the Oxy
 * calls your dev server sends it — attaching the `oxy login` bearer and
 * applying guardrails — to the resolved cloud target.
 *
 * A PORT of `crates/app/src/cli/commands/proxy.rs`, and the guardrails are
 * carried across verbatim rather than reinvented, because each one is a
 * decision somebody argued and every one of them is about not writing to a
 * customer's production data from a laptop:
 *
 *   - side-effecting calls are HELD by default (`--allow-writes` forwards)
 *   - tracking events are DROPPED by default (`--allow-events` forwards)
 *   - auth endpoints reach the backend UNAUTHENTICATED, so sign-in works
 *   - the dev token is a FALLBACK, never an override of a real browser session
 *   - `Set-Cookie` is rewritten so a cloud cookie is storable on localhost
 *
 * The token lives only in this process and is never returned to the browser.
 * Authorization is decided by the cloud; the proxy only forwards.
 */

import { createServer, type IncomingMessage, type ServerResponse } from "node:http";

import type { Context } from "../context/resolve.js";
import * as log from "../ui/log.js";
import { out } from "../ui/tty.js";
import { CliError, ExitCode } from "../util/errors.js";

export interface ProxyFlags {
  port?: string;
  allowWrites?: boolean;
  allowEvents?: boolean;
  yes?: boolean;
}

/**
 * Cap on a buffered request body. Requests through the proxy are small (query
 * and semantic-query JSON); the large payloads are RESPONSES, which stream.
 */
const MAX_REQUEST_BODY = 25 * 1024 * 1024;

/**
 * Headers this proxy must not forward.
 *
 * The first eight are hop-by-hop by definition. `content-encoding` and
 * `content-range` are here for a different and less obvious reason: node's
 * `fetch` is not `reqwest`. undici sends its own
 * `accept-encoding: gzip, deflate, br` and DECODES the response transparently,
 * so by the time the body reaches this code it is plain bytes — forwarding the
 * upstream's `content-encoding: gzip` labels them as compressed and the
 * browser fails with `ERR_CONTENT_DECODING_FAILED` on exactly the
 * `POST /projects/{id}/query` round-trip this command exists for.
 *
 * The Rust original is safe by omission rather than by design: its `reqwest`
 * is built without the `gzip`/`brotli` features, so it never asks for a
 * compressed body and never had the case.
 */
const HOP_BY_HOP = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
  "content-length",
  "content-encoding",
  // Safe to strip only because `range` is not among the headers
  // `buildRequestHeaders` forwards, so upstream never answers 206. Unlike
  // `content-encoding`, undici does not rewrite this one — adding `range` to
  // that forward list without removing this entry would silently break every
  // partial response.
  "content-range"
]);

/**
 * `POST /api/customer-apps/{id}/events` — usage tracking. Dropped by default
 * so local dev never writes to the target's analytics.
 */
export function isEventsPath(path: string): boolean {
  const bare = path.split("?")[0] ?? path;
  return bare.startsWith("/api/customer-apps/") && bare.endsWith("/events");
}

/**
 * Auth / session endpoints. These must reach the backend UNAUTHENTICATED to
 * establish a session, so the proxy never injects the dev bearer on them and
 * never holds them behind `--allow-writes`.
 */
export function isAuthPath(path: string): boolean {
  const bare = path.split("?")[0] ?? path;
  return bare.startsWith("/api/auth/") || bare === "/api/user";
}

/**
 * Whether a request should be HELD.
 *
 * ALLOWLIST, NOT DENYLIST: any mutating method is held EXCEPT the two
 * POST-but-read data-plane endpoints, which carry their filter in the body.
 * GET/HEAD are never mutating; events and auth have their own handling.
 */
export function isWritePath(method: string, path: string): boolean {
  const mutating = ["POST", "PUT", "PATCH", "DELETE"].includes(method.toUpperCase());
  if (!mutating || isEventsPath(path) || isAuthPath(path)) return false;
  const bare = path.split("?")[0] ?? path;
  const readPost =
    method.toUpperCase() === "POST" &&
    (bare.endsWith("/query") || bare.endsWith("/semantic-query"));
  return !readPost;
}

/**
 * Make a cloud backend's `Set-Cookie` storable by a browser on `localhost`.
 *
 * The standard dev-proxy rewrite: strip `Domain=` (which would scope the
 * cookie to the cloud host) and `Secure` (which a plain-http localhost will
 * not store), and relax `SameSite=None`, which browsers only honour together
 * with `Secure`.
 */
export function rewriteSetCookie(value: string): string {
  return value
    .split(";")
    .map((part) => part.trim())
    .filter((part) => !/^domain=/i.test(part) && !/^secure$/i.test(part))
    .map((part) => (/^samesite=none$/i.test(part) ? "SameSite=Lax" : part))
    .join("; ");
}

/**
 * Headers to send upstream.
 *
 * The browser's own auth is forwarded transparently so a signed-in session
 * works; `Origin` and `Referer` go too, because the backend derives its base
 * URL from them and an OAuth `redirect_uri` that does not match what the
 * provider issued the code for is a 401 at sign-in.
 *
 * The dev bearer is injected ONLY as a fallback — when the request carries no
 * auth of its own and is not an auth endpoint — so it can never override a
 * real session or break login.
 */
export function buildRequestHeaders(
  incoming: NodeJS.Dict<string | string[]>,
  token: string | undefined,
  path: string
): Record<string, string> {
  const headers: Record<string, string> = {};
  for (const name of ["content-type", "accept", "cookie", "authorization", "origin", "referer"]) {
    const value = incoming[name];
    if (typeof value === "string") headers[name] = value;
    else if (Array.isArray(value) && value[0]) headers[name] = value[0];
  }
  const hasAuth = "cookie" in headers || "authorization" in headers;
  if (!hasAuth && !isAuthPath(path) && token) headers.authorization = `Bearer ${token}`;
  return headers;
}

/** Read a request body, refusing anything past the cap. */
function readBody(req: IncomingMessage): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    req.on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_REQUEST_BODY) {
        reject(new Error(`request body over ${MAX_REQUEST_BODY} bytes`));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on("end", () => resolve(Buffer.concat(chunks)));
    req.on("error", reject);
  });
}

function json(res: ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(payload)
  });
  res.end(payload);
}

/**
 * Is this target a production deployment?
 *
 * A PURE PREDICATE, and exported, because it is the one guardrail this port
 * adds on top of the Rust — so it has no prior art to lean on, and a regex is
 * the kind of thing that goes wrong quietly. Testing it through `runProxy`
 * would mean starting a server that never exits for every negative case.
 *
 * Matched on the HOST, after `parseEnvUrl` has already canonicalised an org
 * subdomain (`acme.oxygen-hq.com`) to the product host — so naming a customer's
 * own subdomain is refused too.
 *
 * NARROWER THAN THE RUST IN ONE CASE, deliberately: `proxy.rs` also refuses on
 * the env NAME, so a manifest mapping `production` to a non-prod URL is refused
 * there and allowed here. Here the URL is the truth — an `oxy-app.json` that
 * deliberately points `production` somewhere else is a statement about where
 * production is, and refusing it would make the manifest unusable.
 */
export function isProductionTarget(target: string): boolean {
  let host: string;
  try {
    host = new URL(target).hostname.toLowerCase();
  } catch {
    // Unparseable: treat as production. The guard exists to prevent an
    // accident, and "I could not tell" is not a reason to allow one.
    return true;
  }
  // The apex is matched explicitly: `"oxygen-hq.com".endsWith(".oxygen-hq.com")`
  // is false, and the regex this replaced did catch the apex. `app.` needs no
  // arm of its own — the suffix already subsumes it.
  return host === "oxygen-hq.com" || host.endsWith(".oxygen-hq.com");
}

export async function runProxy(ctx: Context, flags: ProxyFlags): Promise<void> {
  const target = ctx.target();
  const port = Number(flags.port ?? 3000);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new CliError(`invalid --port ${flags.port}`, { code: ExitCode.USAGE });
  }

  // A production target is confirmed, not assumed. The guardrails hold writes
  // by default, but `--allow-writes` plus a forgotten `--env` is a laptop
  // writing to a customer's live data.
  if (isProductionTarget(target) && !flags.yes) {
    throw new CliError(`refusing to proxy to a production target (${target}) without --yes`, {
      code: ExitCode.REFUSED,
      hint: "oxyc proxy --env dev        — or pass --yes if production is really what you want"
    });
  }

  const token = ctx.maybeBearer();
  if (!token) {
    log.warn(`no cached token for ${target} — only requests carrying their own auth will work`);
    log.hint(`oxyc login --env ${ctx.flags.env ?? "production"}`);
  }

  const server = createServer(async (req, res) => {
    const path = req.url ?? "/";
    const method = (req.method ?? "GET").toUpperCase();

    if (!flags.allowEvents && isEventsPath(path)) {
      // DROPPED, and answered 204 rather than refused: the SDK's tracking call
      // is fire-and-forget, and a 4xx would surface in the app's console as a
      // bug that is not one.
      log.info(`${out.dim("dropped")}  ${method} ${path}  (tracking; --allow-events to forward)`);
      res.writeHead(204).end();
      return;
    }

    if (!flags.allowWrites && isWritePath(method, path)) {
      // HELD with a 403 and a body that says WHY. A silent 200 would let a
      // dev believe a write landed in the cloud when nothing happened.
      log.warn(`held  ${method} ${path}  (side-effecting; --allow-writes to forward)`);
      // 409, NOT 403, and the status is chosen to MISS the auth-shaped ones.
      // `@oxy-hq/sdk`'s error interpreter matches `^403:` as its catch-all and
      // renders "Access denied — check the oxy server logs", which discards
      // the body written here specifically to say why and sends the developer
      // to a server's logs for something their own laptop did. 409 has no
      // branch there, so the message survives. The Rust original returns 409
      // for the same reason.
      json(res, 409, {
        error: "held_by_oxyc_proxy",
        message: `oxyc proxy holds side-effecting calls by default. Re-run with --allow-writes to forward ${method} ${path}.`
      });
      return;
    }

    let body: Buffer;
    try {
      body = await readBody(req);
    } catch (cause) {
      json(res, 413, { error: "request_too_large", message: (cause as Error).message });
      return;
    }

    try {
      const upstream = await fetch(`${target.replace(/\/+$/, "")}${path}`, {
        method,
        headers: buildRequestHeaders(req.headers, token, path),
        body: method === "GET" || method === "HEAD" ? undefined : body,
        redirect: "manual"
      });

      const outHeaders: Record<string, string | string[]> = {};
      upstream.headers.forEach((value, name) => {
        if (HOP_BY_HOP.has(name.toLowerCase())) return;
        outHeaders[name] = name.toLowerCase() === "set-cookie" ? rewriteSetCookie(value) : value;
      });
      // `getSetCookie` keeps multiple cookies separate; the iteration above
      // collapses them, and a login that sets two would lose one.
      const cookies = upstream.headers.getSetCookie?.() ?? [];
      if (cookies.length > 0) outHeaders["set-cookie"] = cookies.map(rewriteSetCookie);

      res.writeHead(upstream.status, outHeaders);
      if (upstream.body) {
        // Streamed, not buffered: responses are the large payloads here, and
        // an SSE stream buffered to completion never arrives at all.
        for await (const chunk of upstream.body as unknown as AsyncIterable<Uint8Array>) {
          res.write(chunk);
        }
      }
      res.end();
      log.info(`${upstream.status}  ${method} ${path}`);
    } catch (cause) {
      log.error(`${method} ${path} failed: ${(cause as Error).message}`);
      json(res, 502, { error: "upstream_unreachable", message: (cause as Error).message });
    }
  });

  await new Promise<void>((resolve, reject) => {
    server.once("error", (cause: NodeJS.ErrnoException) => {
      reject(
        cause.code === "EADDRINUSE"
          ? new CliError(`port ${port} is already in use`, {
              code: ExitCode.UNAVAILABLE,
              hint: "a local `oxy serve` is probably on it — stop it, or pass --port"
            })
          : cause
      );
    });
    server.listen(port, "127.0.0.1", resolve);
  });

  process.stderr.write(
    `${out.green(`oxyc proxy → ${target}`)}\n` +
      `  listening on http://127.0.0.1:${port}\n` +
      `  writes ${flags.allowWrites ? out.yellow("FORWARDED") : "held"}` +
      `   ·   events ${flags.allowEvents ? out.yellow("FORWARDED") : "dropped"}\n`
  );

  // Runs until interrupted; the process is the server.
  await new Promise<void>(() => {});
}
