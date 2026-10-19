const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/**
 * RFC 4122 shape check for an id that arrived as text — a query param, a pasted
 * value — before it is allowed to name a resource in a URL. The server would 404
 * a malformed id anyway; the point is that `../` never gets as far as a request.
 */
export const isUuid = (s: string | null | undefined): s is string => !!s && UUID_RE.test(s.trim());
