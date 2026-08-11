/**
 * Binary units, matching how the object store and the server-side ceilings
 * (`INLINE_BLOB_MAX_BYTES`, `OXY_CUSTOMER_APPS_STORAGE_MAX_UPLOAD_BYTES`) are
 * all stated. Mixing SI and binary across a UI that also shows a quota is how
 * "why does 5 GB show as 4.7?" tickets get filed.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "—";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const negative = bytes < 0;
  let value = Math.abs(bytes);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const sign = negative ? "-" : "";
  return unit === 0
    ? `${sign}${Math.round(value)} ${units[0]}`
    : `${sign}${value.toFixed(1)} ${units[unit]}`;
}

/** Signed, for a growth column where direction is the point. */
export function formatDelta(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes === 0) return "0";
  return `${bytes > 0 ? "↑" : "↓"} ${formatBytes(Math.abs(bytes))}`;
}

export function formatRelative(iso: string | null): string {
  if (!iso) return "never";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const seconds = Math.floor((Date.now() - then) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/**
 * Days until a key expires, from its TTL class and last-modified time.
 *
 * Derived rather than read back from S3: `GetObjectTagging` is one request per
 * object, which would dwarf the listing itself. It reflects **today's** policy,
 * so an object written under an older rule may show a class it doesn't yet
 * carry — which is the more useful answer anyway ("what will happen to this
 * prefix from now on").
 *
 * `null` when the key never expires or the class is unrecognized.
 */
export function daysUntilExpiry(expireAfter: string | undefined, lastModified: string | null) {
  if (!expireAfter || !lastModified) return null;
  const days = Number.parseInt(expireAfter.replace(/d$/, ""), 10);
  if (!Number.isFinite(days)) return null;
  const written = new Date(lastModified).getTime();
  if (Number.isNaN(written)) return null;
  const elapsed = (Date.now() - written) / 86_400_000;
  // S3 evaluates tag-filtered rules daily, so this is approximate by design.
  return Math.max(0, Math.ceil(days - elapsed));
}
