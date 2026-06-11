/** Compact USD: $0.0042 / $3.21 / $1.2k / $4.5M. Cost spans many orders of
 *  magnitude on this dashboard, so the formatter adapts precision to scale. */
export function usd(n: number): string {
  if (!Number.isFinite(n)) return "$0";
  if (n === 0) return "$0";
  if (n < 0.01) return `$${n.toFixed(4)}`;
  if (n < 1000) return `$${n.toFixed(2)}`;
  if (n < 1_000_000) return `$${(n / 1000).toFixed(1)}k`;
  return `$${(n / 1_000_000).toFixed(2)}M`;
}

/** Compact token / run counts: 1.2k / 3.4M / 1.1B. */
export function compact(n: number): string {
  if (!Number.isFinite(n)) return "0";
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  if (n < 1_000_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  return `${(n / 1_000_000_000).toFixed(2)}B`;
}

/** Short model label — strips the vendor's date suffix for legibility. */
export function shortModel(model: string): string {
  return model
    .replace(/^(us\.)?(anthropic\.)?/, "")
    .replace(/-\d{8}$/, "")
    .replace(/-latest$/, "");
}
