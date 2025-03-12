import { apiBaseURL } from "@/services/env";

export type LogType = "info" | "error" | "warning" | "success";

export type LogItem = {
  timestamp: string;
  content: string;
  log_type: LogType;
};

async function* logItemGenerator(
  reader: ReadableStreamDefaultReader<Uint8Array<ArrayBufferLike>>,
  decoder: TextDecoder,
) {
  let buffer = "";
  while (true) {
    const { value, done } = await reader.read();
    if (done) return;

    buffer += decoder.decode(value, { stream: true });

    const parts = buffer.split("\n"); // Assume JSON objects are newline-delimited
    buffer = parts.pop() || ""; // Keep the last incomplete part for the next chunk

    for (const part of parts) {
      if (!part.trim()) continue;
      try {
        const json = JSON.parse(part);
        yield json;
      } catch (e) {
        console.error("JSON parse error:", e);
      }
    }
  }
}

async function runWorkflow({ workflowPath }: { workflowPath: string }) {
  const pathBase64 = btoa(workflowPath);
  const apiPath = `${apiBaseURL}/workflows/${encodeURIComponent(pathBase64)}/run`;
  const response = await fetch(apiPath, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
  });
  const reader = response.body?.getReader();
  if (!reader) {
    console.error("Failed to get response reader");
    return;
  }

  const decoder = new TextDecoder();
  return logItemGenerator(reader, decoder);
}

export default runWorkflow;
