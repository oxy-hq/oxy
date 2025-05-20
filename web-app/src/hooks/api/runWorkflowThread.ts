import { apiBaseURL } from "@/services/env";
import { logItemGenerator } from "./runWorkflow";

async function runWorkflowThread({ threadId }: { threadId: string }) {
  const apiPath = `${apiBaseURL}/threads/${threadId}/workflow`;
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

export default runWorkflowThread;
