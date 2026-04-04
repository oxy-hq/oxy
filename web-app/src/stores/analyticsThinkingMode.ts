import type { ThinkingMode } from "@/services/api/analytics";

/**
 * Ephemeral in-memory store to carry the thinking-mode preference chosen in
 * ChatPanel over to the AnalyticsThread that auto-starts on first visit.
 *
 * Keyed by thread ID. Consumed (read + deleted) by AnalyticsThread so entries
 * don't accumulate.
 */
const pending = new Map<string, ThinkingMode>();

export function setPendingThinkingMode(threadId: string, mode: ThinkingMode) {
  if (mode !== "auto") {
    pending.set(threadId, mode);
  }
}

export function consumePendingThinkingMode(threadId: string): ThinkingMode | null {
  const mode = pending.get(threadId) ?? null;
  pending.delete(threadId);
  return mode;
}
