import type { WaterfallSpan } from "@/services/api/traces";

export function getTimelineSpanColor(
  spanName: string,
  statusCode: string,
): string {
  if (statusCode === "ERROR") return "bg-destructive";
  if (spanName.includes("llm")) return "bg-purple-500";
  if (spanName.includes("tool")) return "bg-amber-500";
  if (spanName.includes("agent")) return "bg-blue-500";
  if (spanName.includes("context")) return "bg-green-500";
  return "bg-primary";
}

// Recursively parse nested JSON strings
export function deepParseJson(obj: unknown): unknown {
  if (typeof obj === "string") {
    try {
      const parsed = JSON.parse(obj);
      return deepParseJson(parsed);
    } catch {
      return obj;
    }
  }
  if (Array.isArray(obj)) {
    return obj.map(deepParseJson);
  }
  if (obj !== null && typeof obj === "object") {
    const result: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(obj)) {
      result[key] = deepParseJson(val);
    }
    return result;
  }
  return obj;
}

export function getSpanIcon(spanName: string): string {
  // LLM calls
  if (spanName.startsWith("llm.")) return "🤖";
  // Tool execution
  if (spanName === "tool_call.execute") return "⚡";
  if (spanName.startsWith("tool.")) return "🔧";
  // Agent operations - specific agents first
  if (spanName.includes("routing_agent")) return "🔀";
  if (spanName.includes("fallback_agent")) return "🔄";
  if (spanName.includes("default_agent")) return "🎯";
  if (spanName === "agent.run_agent") return "🚀";
  if (spanName === "agent.get_global_context") return "🌐";
  if (spanName === "agent.load_config") return "⚙️";
  if (spanName === "agent.execute") return "▶️";
  if (spanName.startsWith("agent.")) return "🎯";
  // Data operations
  if (spanName === "load") return "📥";
  if (spanName === "count_rows") return "🔢";
  return "📊";
}

export function getSpanColor(span: WaterfallSpan): string {
  if (span.statusCode === "ERROR") return "bg-destructive";
  const spanName = span.spanName;
  // LLM calls - purple
  if (spanName.startsWith("llm.")) return "bg-purple-500";
  // Tool execution - amber
  if (spanName === "tool_call.execute") return "bg-orange-500";
  if (spanName.startsWith("tool.")) return "bg-amber-500";
  // Agent operations - various blues
  if (spanName.includes("routing_agent")) return "bg-cyan-500";
  if (spanName.includes("fallback_agent")) return "bg-slate-500";
  if (spanName.includes("default_agent")) return "bg-blue-500";
  if (spanName === "agent.run_agent") return "bg-indigo-600";
  if (spanName === "agent.get_global_context") return "bg-green-500";
  if (spanName === "agent.load_config") return "bg-blue-400";
  if (spanName.startsWith("agent.")) return "bg-blue-500";
  // Data operations - teal
  if (spanName === "load" || spanName === "count_rows") return "bg-teal-500";
  return "bg-primary";
}
