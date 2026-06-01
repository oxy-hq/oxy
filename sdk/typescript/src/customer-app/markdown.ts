// Markdown helpers used by `<OxyAnswer>`.
//
// Extracted from `react.tsx` so they're testable without spinning up
// a React renderer in unit tests. The renderer itself stays in
// `react.tsx` because it's JSX-heavy.

/**
 * Allowlist for `[text](url)` href values in agent-emitted markdown.
 * Markdown comes from an LLM, which sits across an external trust
 * boundary — without this filter, a `javascript:` URL produced by
 * the model would render as a clickable XSS in the bundle's origin.
 *
 * Accepts:
 *   - http(s):// absolute URLs
 *   - mailto: addresses
 *   - root-relative paths (`/foo`)
 *   - same-page fragments (`#section`)
 *
 * Rejects everything else, including `javascript:`, `data:`,
 * protocol-relative `//evil.com`, and any other scheme. Comparison
 * is case-insensitive after stripping leading whitespace + ASCII
 * control bytes (browsers strip these before scheme resolution, so
 * `java\tscript:` would otherwise slip past a naive prefix check).
 */
export function isSafeLinkHref(raw: string): boolean {
  // Built char-by-char rather than via regex to avoid embedding
  // actual control bytes in source (linters/IDEs mangle them).
  let cleaned = "";
  for (let i = 0; i < raw.length; i++) {
    const cc = raw.charCodeAt(i);
    if (cc > 0x20 && cc !== 0x7f) cleaned += raw[i];
  }
  if (cleaned === "") return false;
  if (cleaned.startsWith("#") || cleaned.startsWith("/")) {
    // Reject protocol-relative (`//host/...`) — resolves against
    // current scheme + host, a classic open-redirect vector.
    if (cleaned.startsWith("//")) return false;
    return true;
  }
  const lower = cleaned.toLowerCase();
  return lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("mailto:");
}
