#!/usr/bin/env node
// OAuth bounce proxy for local multi-instance development.
//
// Problem: OAuth providers validate the `redirect_uri` against a fixed
// allow-list, so every dev port you run on would otherwise need its own
// registered URI.
//
// Solution: register ONE redirect URI per provider with this proxy's origin
// (e.g. http://localhost:8429/auth/google/callback and
// http://localhost:8429/github/callback) and run every Oxy dev instance on its
// own port pointed at this proxy (OXY_OAUTH_PROXY_ORIGIN +
// OXY_OAUTH_REDIRECT_ORIGIN). The instance appends its own origin to the OAuth
// `state`; this proxy reads it and 302-redirects the provider's callback to the
// instance that started the flow, preserving the entire query (code, state,
// installation_id, …). The instance's session cookie is set on its own origin,
// so multiple instances stay isolated.
//
// GitHub note: register this origin as a callback URL on BOTH the GitHub OAuth
// app and the GitHub App (GitHub Apps accept multiple callback URLs).
//
// Run: node scripts/oauth-bounce.mjs   (PORT via OXY_OAUTH_PROXY_PORT, default 8429)

import http from "node:http";

const PORT = Number(process.env.OXY_OAUTH_PROXY_PORT) || 8429;
// Provider callback paths this proxy will bounce. Google uses a full-page
// redirect; GitHub (login, account-connect, and App-install) uses a popup —
// both just need the same 302 to the originating instance.
const CALLBACK_PATHS = new Set(["/auth/google/callback", "/github/callback"]);
const STATE_ORIGIN_SEP = "~";

/** Decode the base64url-encoded instance origin appended to `state`. */
function originFromState(state) {
  if (!state) return null;
  const i = state.indexOf(STATE_ORIGIN_SEP);
  if (i === -1) return null;
  const b64 = state.slice(i + 1).replace(/-/g, "+").replace(/_/g, "/");
  try {
    return Buffer.from(b64, "base64").toString("utf8");
  } catch {
    return null;
  }
}

/** Only bounce to loopback origins — prevents this from being an open redirect. */
function isLoopbackOrigin(origin) {
  try {
    const u = new URL(origin);
    return (
      (u.protocol === "http:" || u.protocol === "https:") &&
      (u.hostname === "localhost" || u.hostname === "127.0.0.1")
    );
  } catch {
    return false;
  }
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://localhost:${PORT}`);

  if (!CALLBACK_PATHS.has(url.pathname)) {
    res.writeHead(404, { "content-type": "text/plain" });
    res.end(
      "oauth-bounce: only " + [...CALLBACK_PATHS].join(", ") + " are handled\n"
    );
    return;
  }

  const origin = originFromState(url.searchParams.get("state"));
  if (!origin || !isLoopbackOrigin(origin)) {
    res.writeHead(400, { "content-type": "text/plain" });
    res.end("oauth-bounce: missing/invalid instance origin in state\n");
    console.error(`[oauth-bounce] rejected callback; origin=${origin}`);
    return;
  }

  // Forward the entire original query (code, state, scope, installation_id, …)
  // on the same path so the SPA's callback page sees exactly what the provider
  // returned.
  const target = `${origin}${url.pathname}${url.search}`;
  res.writeHead(302, { Location: target });
  res.end();
  console.log(`[oauth-bounce] → ${origin}${url.pathname}`);
});

server.listen(PORT, () => {
  const paths = [...CALLBACK_PATHS]
    .map((p) => `http://localhost:${PORT}${p}`)
    .join("\n[oauth-bounce]   ");
  console.log(
    `[oauth-bounce] listening; register these callback URIs with the providers:\n` +
      `[oauth-bounce]   ${paths}\n` +
      "[oauth-bounce] point instances at it via OXY_OAUTH_PROXY_ORIGIN + OXY_OAUTH_REDIRECT_ORIGIN."
  );
});
