// Shared helpers for the local OAuth bounce proxy (scripts/oauth-bounce.mjs).
//
// Several local dev instances on different ports share ONE provider-registered
// redirect URI (the proxy's origin). Each instance appends its own origin to the
// OAuth `state`; the proxy reads it and forwards the callback back to the
// instance that started the flow. Used by both Google and GitHub sign-in.

// Origin of the local OAuth bounce proxy, injected by vite.config.ts from the
// repo-root .env (`OXY_OAUTH_PROXY_ORIGIN`). Empty in normal/prod builds.
declare const __OXY_OAUTH_PROXY_ORIGIN__: string;
export const OAUTH_PROXY_ORIGIN = __OXY_OAUTH_PROXY_ORIGIN__;

// Separator between the CSRF state token and the base64url-encoded instance
// origin appended for the bounce proxy. Neither a JWT, an HMAC body, nor
// base64url contains `~`, so the split is unambiguous.
const STATE_ORIGIN_SEP = "~";

/** Append this instance's origin to the OAuth `state` so the bounce proxy knows
 *  where to forward the callback. No-op when the proxy is disabled. */
export function encodeOAuthState(stateToken: string): string {
  if (!OAUTH_PROXY_ORIGIN) return stateToken;
  const originB64 = btoa(window.location.origin)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return `${stateToken}${STATE_ORIGIN_SEP}${originB64}`;
}

/** Strip the appended origin, returning the bare CSRF token the backend expects. */
export function oauthStateToken(state: string): string {
  const i = state.indexOf(STATE_ORIGIN_SEP);
  return i === -1 ? state : state.slice(0, i);
}
