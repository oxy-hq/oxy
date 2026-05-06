import { useMemo } from "react";
import type { UiBlock } from "@/services/api/analytics";

// ── Types ─────────────────────────────────────────────────────────────────────

export type BuilderToolActivity = {
  kind: "tool_used";
  id: string;
  toolName: string;
  summary: string;
};

export type BuilderFileChange = {
  kind: "file_changed";
  id: string;
  filePath: string;
  description: string;
  newContent: string;
  /** Old content extracted from the corresponding awaiting_input prompt JSON. */
  oldContent: string;
  isDeletion: boolean;
  status: "pending" | "accepted" | "rejected";
};

export type BuilderActivityItem = BuilderToolActivity | BuilderFileChange;

// ── Hook ──────────────────────────────────────────────────────────────────────

/**
 * Derives an ordered list of builder activity items from the SSE event stream.
 *
 * @param events        The full UiBlock event list for a run.
 * @param changeDecisions  Map from `file_changed` event seq → accept/reject decision.
 */
export function useBuilderActivity(
  events: UiBlock[],
  changeDecisions: Map<number, "accepted" | "rejected">
): BuilderActivityItem[] {
  return useMemo(() => {
    const items: BuilderActivityItem[] = [];

    for (const ev of events) {
      if (ev.event_type === "tool_used") {
        items.push({
          kind: "tool_used",
          id: `builder-tool-${ev.seq}`,
          toolName: ev.payload.tool_name,
          summary: ev.payload.summary
        });
      } else if (ev.event_type === "file_change_pending") {
        // Check explicit decisions first, then fall back to scanning events
        // for input_resolved (handles page reload / hydration).
        const decision =
          changeDecisions.get(ev.seq) ?? extractChangeDecision(events, ev.seq) ?? "pending";
        // old_content is now included directly in the event payload; fall back
        // to extractFileChangedMetadata for backwards-compat with older runs.
        const oldContent =
          ev.payload.old_content !== undefined && ev.payload.old_content !== ""
            ? ev.payload.old_content
            : extractFileChangedMetadata(events, ev.seq).oldContent;
        const isDeletion =
          ev.payload.delete ?? extractFileChangedMetadata(events, ev.seq).isDeletion;
        items.push({
          kind: "file_changed",
          id: `builder-change-${ev.seq}`,
          filePath: ev.payload.file_path,
          description: ev.payload.description,
          newContent: ev.payload.new_content,
          oldContent,
          isDeletion,
          status: decision
        });
      } else if (ev.event_type === "file_changed") {
        // file_changed events are emitted after user acceptance — always "accepted".
        items.push({
          kind: "file_changed",
          id: `builder-change-${ev.seq}`,
          filePath: ev.payload.file_path,
          description: ev.payload.description,
          newContent: ev.payload.new_content,
          oldContent: ev.payload.old_content,
          isDeletion: ev.payload.is_deletion,
          status: "accepted"
        });
      }
    }

    return items;
  }, [events, changeDecisions]);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Finds the `awaiting_input` event that follows a `file_changed` event and
 * parses metadata from its JSON prompt. Returns defaults on failure.
 */
export function extractFileChangedMetadata(
  events: UiBlock[],
  afterSeq: number
): { oldContent: string; isDeletion: boolean } {
  for (const ev of events) {
    if (ev.seq <= afterSeq) continue;
    if (ev.event_type === "awaiting_input") {
      const prompt = ev.payload.questions[0]?.prompt ?? "";
      try {
        const parsed = JSON.parse(prompt);
        if (
          parsed?.type === "file_change" ||
          parsed?.type === "write_file" ||
          parsed?.type === "edit_file" ||
          parsed?.type === "delete_file"
        ) {
          return {
            oldContent: typeof parsed.old_content === "string" ? parsed.old_content : "",
            isDeletion: parsed.delete === true || parsed.type === "delete_file"
          };
        }
      } catch {
        // not JSON
      }
      break;
    }
  }
  return { oldContent: "", isDeletion: false };
}

export function extractOldContent(events: UiBlock[], afterSeq: number): string {
  return extractFileChangedMetadata(events, afterSeq).oldContent;
}

/**
 * Finds the `input_resolved` event that follows a `file_changed` event
 * and determines whether the change was accepted or rejected based on the answer.
 */
export function extractChangeDecision(
  events: UiBlock[],
  afterSeq: number
): "accepted" | "rejected" | null {
  for (const ev of events) {
    if (ev.seq <= afterSeq) continue;
    if (ev.event_type === "input_resolved") {
      const answer = (ev.payload as { answer?: string }).answer ?? "";
      return answer.toLowerCase().includes("accept") ? "accepted" : "rejected";
    }
  }
  return null;
}
