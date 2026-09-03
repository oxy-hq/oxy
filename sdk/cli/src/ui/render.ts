/**
 * The one place a table, a paragraph or a status line becomes bytes.
 *
 * The two audiences want genuinely different things and neither is a
 * degraded version of the other: a human wants aligned columns and colour, an
 * agent wants a markdown table — which LLMs read far more reliably than either
 * ASCII art or JSON, and at a fraction of JSON's token cost, because a JSON
 * array repeats every field name on every row.
 *
 * So the *caller* never picks a format. It hands over rows and columns; this
 * decides, from `stdoutIsTty()`, and there is exactly one such decision in the
 * codebase.
 */

import { out, stdoutIsTty, terminalWidth } from "./tty.js";

export interface Column<T> {
  header: string;
  /** The cell, already stringified. Empty string for "nothing to show". */
  value: (row: T) => string;
  /** Right-align (counts, sizes). Ignored in markdown output. */
  align?: "left" | "right";
}

/**
 * Render `rows` as a table on stdout: aligned + coloured for a terminal,
 * GitHub-flavoured markdown for anything else.
 *
 * An empty `rows` prints nothing at all rather than a header with no body. A
 * bare header reads as a claim that the set was inspected and found empty,
 * which is only sometimes what happened — the caller says that in words, where
 * it can also say *why*.
 */
export function table<T>(rows: T[], columns: Column<T>[]): string {
  if (rows.length === 0) return "";
  // Each path escapes for itself: markdown needs the pipe escaped, a terminal
  // does not and is one column wider per pipe if it gets it.
  return stdoutIsTty()
    ? alignedTable(
        rows.map((r) => columns.map((c) => oneLine(c.value(r)))),
        columns
      )
    : markdownTable(
        rows.map((r) => columns.map((c) => forMarkdown(c.value(r)))),
        columns
      );
}

/**
 * Collapse anything that would break a ROW into a single line.
 *
 * A newline turns one table into a table and some loose prose; a tab is
 * squeezed for the same reason the bash tooling squeezes it out of a
 * description — the format has to hold for any value the upstream will accept,
 * not just the well-behaved ones. Both paths need this.
 */
function oneLine(value: string): string {
  return value.replace(/[\r\n\t]+/g, " ");
}

/**
 * …and, for markdown only, escape the delimiter.
 *
 * SPLIT FROM `oneLine` because a pipe is a MARKDOWN concern. Running the
 * aligned path through the escape too meant a terminal header from
 * `SELECT 1 AS "a|b"` printed as `a\|b` — two characters wider than the value
 * it measured, so every column after it sat one place out.
 */
function forMarkdown(value: string): string {
  return oneLine(value).replace(/\|/g, "\\|");
}

function alignedTable<T>(cells: string[][], columns: Column<T>[]): string {
  // Same reason as the markdown path: a header carrying a newline would break
  // the alignment for every row below it.
  const headers = columns.map((c) => oneLine(c.header));
  const widths = columns.map((_c, i) =>
    Math.max(visibleWidth(headers[i] ?? ""), ...cells.map((row) => visibleWidth(row[i] ?? "")))
  );
  const line = (values: string[], style: (s: string) => string) =>
    values
      .map((v, i) => {
        const w = widths[i] ?? 0;
        const padded = columns[i]?.align === "right" ? v.padStart(w) : v.padEnd(w);
        // The last column is never padded: trailing spaces are invisible and
        // they make a copied line carry junk.
        return style(i === values.length - 1 ? v : padded);
      })
      .join("  ")
      .trimEnd();

  const header = line(headers, out.bold);
  const body = cells.map((row) => line(row, (s) => s));
  return [header, ...body].join("\n");
}

function markdownTable<T>(cells: string[][], columns: Column<T>[]): string {
  // HEADERS ARE SANITIZED TOO, and that is not symmetry for its own sake.
  // `--md` derives its column names from RESPONSE DATA — the keys of a JSON
  // object, or the header row of a SQL result — so a query as ordinary as
  //     SELECT 1 AS "a | b"
  // puts a pipe in a header and splits every row of the rendered table onto
  // the wrong columns. A newline does worse. Cells have always been cleaned;
  // headers were not, because when this was written they were only ever
  // literals in our own source.
  const header = `| ${columns.map((c) => forMarkdown(c.header)).join(" | ")} |`;
  const rule = `| ${columns.map(() => "---").join(" | ")} |`;
  const body = cells.map((row) => `| ${row.join(" | ")} |`);
  return [header, rule, ...body].join("\n");
}

/**
 * Display width, ignoring ANSI escapes.
 *
 * Values reaching a table are usually plain, but a caller is allowed to colour
 * one (a red `unhealthy`), and counting the escape bytes as width would push
 * every following column out by exactly the length of the escape sequence —
 * a misalignment that only appears once something is coloured.
 */
function visibleWidth(s: string): number {
  // biome-ignore lint/suspicious/noControlCharactersInRegex: matching ANSI escapes is the point
  return s.replace(/\[[0-9;]*m/g, "").length;
}

/** Greedy word wrap. Long prose in a fixed-width column reads better wrapped. */
export function wrap(text: string, width = terminalWidth() - 10): string[] {
  if (!text) return [];
  const lines: string[] = [];
  let current = "";
  for (const word of text.split(/\s+/).filter(Boolean)) {
    if (current && current.length + 1 + word.length > width) {
      lines.push(current);
      current = "";
    }
    current = current ? `${current} ${word}` : word;
  }
  if (current) lines.push(current);
  return lines;
}

/**
 * A heading, as a section separator.
 *
 * Markdown when piped, so an agent reading a multi-section report can tell
 * where one section ends; bold-and-blank-line for a human, because `##` in a
 * terminal is noise.
 */
export function heading(text: string): string {
  return stdoutIsTty() ? `\n${out.bold(text)}` : `\n## ${text}\n`;
}
