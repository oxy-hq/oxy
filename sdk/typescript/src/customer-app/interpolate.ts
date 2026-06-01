/**
 * Interpolate `{{ params.X }}` and `{{ params.X | sqlquote }}` placeholders
 * in a SQL template.
 *
 * - `{{ params.X | sqlquote }}` — quote strings ('foo'), pass numbers and
 *   booleans raw, nullish becomes NULL. Mirrors the server's Jinja sqlquote
 *   filter.
 * - `{{ params.X }}` — raw pass-through. Used for already-trusted values
 *   (numbers, identifiers the caller has validated). Caller is responsible
 *   for safety.
 *
 * Not a security boundary. The server still gates SQL execution by
 * project membership. Bundles that accept untrusted user input should
 * use `| sqlquote` or validate/coerce before passing.
 */
export function interpolateSqlParams(
  sql: string,
  params: Record<string, string | number | boolean | null | undefined>
): string {
  return sql.replace(
    /\{\{\s*params\.([a-zA-Z0-9_]+)(\s*\|\s*sqlquote)?\s*\}\}/g,
    (_match, key: string, sqlquote: string | undefined) => {
      const v = params[key];
      if (v === null || v === undefined) return "NULL";
      if (sqlquote) {
        if (typeof v === "number" || typeof v === "boolean") return String(v);
        return `'${String(v).replace(/'/g, "''")}'`;
      }
      // No filter — raw pass-through. Caller is responsible.
      return String(v);
    }
  );
}
