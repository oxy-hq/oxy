export function getTimelineSpanColor(spanName: string, statusCode: string): string {
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
