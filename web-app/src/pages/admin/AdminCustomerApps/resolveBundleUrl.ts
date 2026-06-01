/**
 * Rewrite a customer-app's canonical `url` (which is absolute and
 * baked at the oxy server's host) to use the SPA's own origin. In
 * local dev that routes the navigation through Vite's `/customer-apps`
 * proxy, which carries the existing `oxy_session` cookie set on
 * `:5173` instead of making the browser hit `:3000` directly (where
 * no cookie exists and the operator gets bounced to login).
 *
 * In production where admin UI + bundles share an origin (e.g.
 * `app.oxy.tech`), or where `OXY_SESSION_COOKIE_DOMAIN=.oxy.tech`
 * already scopes the cookie across subdomains, the rewrite is a
 * no-op — both hosts resolve to the same string.
 */
export function resolveBundleUrl(canonicalUrl: string): string {
  try {
    const u = new URL(canonicalUrl, window.location.origin);
    u.protocol = window.location.protocol;
    // Set hostname + port separately rather than `host`, so a stray port
    // baked into `canonicalUrl` (e.g. `http://localhost:5173/...` from an
    // older server with the localhost default still in place) is explicitly
    // cleared. `window.location.port` is "" for the scheme's default port.
    u.hostname = window.location.hostname;
    u.port = window.location.port;
    return u.toString();
  } catch {
    return canonicalUrl;
  }
}
