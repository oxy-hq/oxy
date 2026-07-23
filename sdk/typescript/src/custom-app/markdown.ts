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

// ── GFM table parsing (used by the OxyAnswer markdown renderer) ──────────────

/** Unescape GFM cell escapes (`\|` → `|`, `\\` → `\`). */
function unescapeCell(cell: string): string {
  return cell.replace(/\\([|\\])/g, "$1");
}

/** Split a GFM table row into trimmed cells, dropping the outer pipes.
 *  Splits on UNESCAPED `|` only — a `\|` inside a cell is a literal pipe,
 *  not a column separator — then unescapes each cell. */
export function splitTableRow(line: string): string[] {
  const s = line.trim();
  const cells: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    // Keep the escape sequence intact here; `unescapeCell` collapses it below.
    if (ch === "\\" && i + 1 < s.length) {
      cur += ch + s[i + 1];
      i++;
      continue;
    }
    if (ch === "|") {
      cells.push(cur);
      cur = "";
      continue;
    }
    cur += ch;
  }
  cells.push(cur);
  // Drop the empty cells produced by the optional leading/trailing pipes,
  // without eating a legitimately-empty first/last column mid-row.
  if (cells.length > 1 && cells[0].trim() === "") cells.shift();
  if (cells.length > 1 && cells[cells.length - 1].trim() === "") cells.pop();
  return cells.map((c) => unescapeCell(c.trim()));
}

/** A GFM delimiter row: every cell is `-`s with optional leading/trailing `:`. */
export function isTableDelimiter(line: string): boolean {
  if (!line?.includes("-")) return false;
  const cells = splitTableRow(line);
  return cells.length > 0 && cells.every((c) => /^:?-{1,}:?$/.test(c));
}

/** A table starts at `idx` when that line has a pipe and the next line is a
 *  delimiter row. */
export function isTableStart(lines: string[], idx: number): boolean {
  return (lines[idx] ?? "").includes("|") && isTableDelimiter(lines[idx + 1] ?? "");
}
