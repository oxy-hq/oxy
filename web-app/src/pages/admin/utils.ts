// Shared admin-console helpers — the ones more than one admin page needs.
// Page-local helpers stay beside their page (see AdminInternalJobs/utils.ts).

/**
 * Format an ISO timestamp as a relative "5m ago" / "yesterday" string.
 * Returns "never" for null/undefined input.
 */
export function relativeTime(iso: string | null | undefined): string {
  if (!iso) {
    return "never";
  }
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) {
    return iso;
  }
  const seconds = Math.floor((Date.now() - d.getTime()) / 1000);
  if (seconds < 0) {
    return "just now";
  }
  if (seconds < 60) {
    return `${seconds}s ago`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours}h ago`;
  }
  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `${days}d ago`;
  }
  return d.toLocaleDateString();
}
