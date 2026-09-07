import type { MeasureValues } from "@/types/metricTree";

export interface LeverInput {
  nodeId: string;
  /** Exactly what the analyst typed: "11", "+5%", "-3". */
  raw: string;
}

export type LeverError = "not_a_number" | "no_baseline" | "zero_baseline" | "no_change";

export type ResolvedLever =
  | { nodeId: string; delta: number }
  | { nodeId: string; error: LeverError };

/**
 * Turn what the analyst typed into a delta the engine can propagate.
 *
 * Three input modes, distinguished by shape rather than by a mode toggle:
 *   "11"    → absolute target, delta = target − baseline
 *   "+5%"   → percentage of baseline
 *   "+3"    → raw delta, the only mode that works without a baseline
 *
 * An explicit sign is what separates a raw delta from an absolute target, so
 * "3" means "set it to 3" while "+3" means "add 3". This is the one piece of
 * syntax the UI has to teach, and the input's placeholder does that.
 */
export function resolveLever(input: LeverInput, baseline: MeasureValues): ResolvedLever {
  const { nodeId } = input;
  const raw = input.raw.trim();
  if (raw === "") return { nodeId, error: "not_a_number" };

  const isPercent = raw.endsWith("%");
  const body = isPercent ? raw.slice(0, -1).trim() : raw;
  const isSigned = body.startsWith("+") || body.startsWith("-");

  const magnitude = Number(body);
  if (!Number.isFinite(magnitude)) return { nodeId, error: "not_a_number" };

  const current = baseline[nodeId];
  const hasBaseline = Number.isFinite(current);

  if (isPercent) {
    if (!hasBaseline) return { nodeId, error: "no_baseline" };
    if (current === 0) return { nodeId, error: "zero_baseline" };
    const delta = (current * magnitude) / 100;
    return delta === 0 ? { nodeId, error: "no_change" } : { nodeId, delta };
  }

  if (isSigned) {
    return magnitude === 0 ? { nodeId, error: "no_change" } : { nodeId, delta: magnitude };
  }

  if (!hasBaseline) return { nodeId, error: "no_baseline" };
  const delta = magnitude - current;
  return delta === 0 ? { nodeId, error: "no_change" } : { nodeId, delta };
}
