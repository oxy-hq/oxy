// Page-local helpers for the Internal Jobs console. `relativeTime` moved to
// `pages/admin/utils.ts` when the Airhouse console needed it too.

export function formatInterval(secs: number): string {
  if (secs < 60) {
    return `${secs}s`;
  }
  const minutes = Math.floor(secs / 60);
  const rem = secs % 60;
  if (rem === 0) {
    return `${minutes}m`;
  }
  return `${minutes}m${rem}s`;
}
