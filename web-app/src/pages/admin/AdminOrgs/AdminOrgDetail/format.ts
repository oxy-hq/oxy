/**
 * Compact USD for the org-360 stat strip and cost card.
 * "$0" · "$0.42" · "$412" · "$1.2k" · "$3.4M". Returns "—" for nullish so a
 * loading/absent value reads as "unknown", not "$0".
 */
export const usd = (n: number | null | undefined): string => {
  if (n == null) return "—";
  if (n === 0) return "$0";
  if (n < 1) return `$${n.toFixed(2)}`;
  if (n < 1_000) return `$${Math.round(n).toLocaleString()}`;
  if (n < 1_000_000) return `$${(n / 1_000).toFixed(1)}k`;
  return `$${(n / 1_000_000).toFixed(1)}M`;
};

/** Compact integer: 1204 → "1,204", 12000 → "12.0k", 3.4M → "3.4M". */
export const compactInt = (n: number | null | undefined): string => {
  if (n == null) return "—";
  if (n < 10_000) return n.toLocaleString();
  if (n < 1_000_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
};
