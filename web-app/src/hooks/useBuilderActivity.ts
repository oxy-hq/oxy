import { useMemo } from "react";
import type { UiBlock } from "@/services/api/analytics";

// ── Types ─────────────────────────────────────────────────────────────────────

export type BuilderToolActivity = {
  kind: "tool_used";
  id: string;
  toolName: string;
  summary: string;
};

export type BuilderProposedChange = {
  kind: "proposed_change";
  id: string;
  filePath: string;
  description: string;
  newContent: string;
  /** Old content extracted from the corresponding awaiting_input prompt JSON. */
  oldContent: string;
  isDeletion: boolean;
  status: "pending" | "accepted" | "rejected";
};

export type BuilderActivityItem = BuilderToolActivity | BuilderProposedChange;

// ── Hook ──────────────────────────────────────────────────────────────────────

/**
 * Derives an ordered list of builder activity items from the SSE event stream.
 *
 * @param events        The full UiBlock event list for a run.
 * @param changeDecisions  Map from `proposed_change` event seq → accept/reject decision.
 */
export function useBuilderActivity(
  events: UiBlock[],
  changeDecisions: Map<number, "accepted" | "rejected">
): BuilderActivityItem[] {
  return useMemo(() => {
    const items: BuilderActivityItem[] = [];
    let counter = 0;
    const nextId = (prefix: string) => `builder-${prefix}-${counter++}`;

    for (const ev of events) {
      if (ev.event_type === "tool_used") {
        items.push({
          kind: "tool_used",
          id: nextId("tool"),
          toolName: ev.payload.tool_name,
          summary: ev.payload.summary
        });
      } else if (ev.event_type === "proposed_change") {
        // Try to pair with the subsequent awaiting_input to extract old_content.
        // Check explicit decisions first, then fall back to scanning events
        // for input_resolved (handles page reload / hydration).
        const decision =
          changeDecisions.get(ev.seq) ?? extractChangeDecision(events, ev.seq) ?? "pending";
        const { oldContent, isDeletion } = extractProposedChangeMetadata(events, ev.seq);
        items.push({
          kind: "proposed_change",
          id: nextId("change"),
          filePath: ev.payload.file_path,
          description: ev.payload.description,
          newContent: ev.payload.new_content,
          oldContent,
          isDeletion: ev.payload.delete ?? isDeletion,
          status: decision
        });
      } else if (ev.event_type === "file_changed") {
        // file_changed events are emitted after user acceptance — always "accepted".
        items.push({
          kind: "proposed_change",
          id: nextId("change"),
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
 * Finds the `awaiting_input` event that follows a `proposed_change` event and
 * parses metadata from its JSON prompt. Returns defaults on failure.
 */
export function extractProposedChangeMetadata(
  events: UiBlock[],
  afterSeq: number
): { oldContent: string; isDeletion: boolean } {
  for (const ev of events) {
    if (ev.seq <= afterSeq) continue;
    if (ev.event_type === "awaiting_input") {
      const prompt = ev.payload.questions[0]?.prompt ?? "";
      try {
        const parsed = JSON.parse(prompt);
        if (parsed?.type === "propose_change") {
          return {
            oldContent: typeof parsed.old_content === "string" ? parsed.old_content : "",
            isDeletion: parsed.delete === true
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
  return extractProposedChangeMetadata(events, afterSeq).oldContent;
}

/**
 * Finds the `input_resolved` event that follows a `proposed_change` event
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
