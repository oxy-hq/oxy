/**
 * Decode the serialized `TaskSpec` JSON (internally tagged: `{ "type": … }`)
 * into a short list of human-readable `label → value` lines for the debug
 * panel. The point is to answer "what was this job actually doing?" without
 * making the operator read raw JSON — agent id, the question, which automation
 * or pipeline, etc.
 *
 * Unknown shapes fall back to the top-level scalar fields so a new TaskSpec
 * variant still renders something useful before this map is updated.
 */
export interface SpecField {
  label: string;
  value: string;
}

const MAX_VALUE_LEN = 600;

export function summarizeSpec(spec: unknown): { type: string | null; fields: SpecField[] } {
  if (spec === null || typeof spec !== "object") {
    return { type: null, fields: [] };
  }
  const obj = spec as Record<string, unknown>;
  const type = typeof obj.type === "string" ? obj.type : null;

  const fields: SpecField[] = [];
  const push = (label: string, raw: unknown) => {
    const value = stringify(raw);
    if (value) fields.push({ label, value });
  };

  switch (type) {
    case "agent":
      push("Agent", obj.agent_id);
      push("Question", obj.question);
      push("Extra", obj.extra);
      break;
    case "workflow":
      push("Automation", obj.workflow_ref);
      push("Variables", obj.variables);
      push("Retry from run", obj.retry_from_run_id);
      if (obj.cache_enabled) push("Cache", "enabled");
      break;
    case "resume":
      push("Resume run", obj.run_id);
      push("Answer", obj.answer);
      break;
    case "workflow_decision":
      push("Decision for run", obj.run_id);
      break;
    case "airway":
      push("Pipeline", obj.pipeline_ref);
      push("Resources", obj.resources);
      push("Variables", obj.variables);
      break;
    case "custom":
      push("Kind", obj.kind);
      push("Payload", obj.payload);
      break;
    default:
      // Unknown / future variant — surface every top-level scalar so the
      // panel is never empty for a spec we don't have a case for yet.
      for (const [k, v] of Object.entries(obj)) {
        if (k === "type") continue;
        push(k, v);
      }
  }

  return { type, fields };
}

function stringify(raw: unknown): string {
  if (raw === null || raw === undefined) return "";
  if (typeof raw === "string") return truncate(raw);
  if (typeof raw === "number" || typeof raw === "boolean") return String(raw);
  try {
    return truncate(JSON.stringify(raw));
  } catch {
    return "";
  }
}

function truncate(s: string): string {
  return s.length > MAX_VALUE_LEN ? `${s.slice(0, MAX_VALUE_LEN)}…` : s;
}
