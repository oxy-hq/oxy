export function getTimelineSpanColor(spanName: string, statusCode: string): string {
  if (statusCode === "ERROR") return "bg-destructive";
  if (spanName.includes("llm")) return "bg-vis-purple";
  if (spanName.includes("tool")) return "bg-warning";
  if (spanName.includes("agent")) return "bg-info";
  if (spanName.startsWith("analytics.")) return "bg-success";
  if (spanName.includes("context")) return "bg-success";
  return "bg-special";
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
