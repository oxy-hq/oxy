/** The badge text: nothing at zero, capped at "99+". */
export function badgeLabel(count: number): string | null {
  if (count <= 0) return null;
  return count > 99 ? "99+" : String(count);
}
