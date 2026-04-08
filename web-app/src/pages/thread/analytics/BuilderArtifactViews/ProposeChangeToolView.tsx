import BaseMonacoEditor from "@/components/MonacoEditor/BaseMonacoEditor";
import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson } from "../analyticsArtifactHelpers";

type ToolRunEvent = {
  type: string;
  data: unknown;
};

type ProposeChangePayload = {
  type: "propose_change";
  file_path: string;
  old_content: string;
  new_content: string;
  description: string;
  delete?: boolean;
};

function parseProposeChangePrompt(prompt: string): ProposeChangePayload | null {
  try {
    const parsed = JSON.parse(prompt);
    if (parsed?.type === "propose_change") {
      return parsed as ProposeChangePayload;
    }
  } catch {
    // not a propose_change prompt
  }
  return null;
}

function languageFromPath(filePath: string): string {
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    rs: "rust",
    ts: "typescript",
    tsx: "typescript",
    js: "javascript",
    jsx: "javascript",
    py: "python",
    sql: "sql",
    yml: "yaml",
    yaml: "yaml",
    json: "json",
    toml: "toml",
    md: "markdown",
    sh: "shell",
    html: "html",
    css: "css"
  };
  return map[ext] ?? "plaintext";
}

function findProposeChangePayload(
  input: {
    file_path?: string;
    description?: string;
    new_content?: string;
    delete?: boolean;
  } | null,
  runEvents: ToolRunEvent[]
): ProposeChangePayload | null {
  if (!input?.file_path || !input.description) return null;

  return (
    [...runEvents]
      .reverse()
      .filter((event) => event.type === "awaiting_input")
      .flatMap((event) => {
        const questions = (event.data as { questions?: Array<{ prompt: string }> } | undefined)
          ?.questions;
        return questions ?? [];
      })
      .map((question) => parseProposeChangePrompt(question.prompt))
      .find(
        (payload) =>
          payload &&
          payload.file_path === input.file_path &&
          payload.description === input.description &&
          payload.new_content === (input.new_content ?? "")
      ) ?? null
  );
}

function getProposeChangeState(
  input: {
    file_path?: string;
    description?: string;
    new_content?: string;
    delete?: boolean;
  } | null,
  runEvents: ToolRunEvent[]
): "accepted" | "pending" {
  if (!input?.file_path || !input.description || runEvents.length === 0) {
    return "pending";
  }

  const matchingAwaitingIndex = runEvents.findIndex((event) => {
    if (event.type !== "awaiting_input") return false;

    const questions = (event.data as { questions?: Array<{ prompt: string }> } | undefined)
      ?.questions;
    return (questions ?? [])
      .map((question) => parseProposeChangePrompt(question.prompt))
      .some(
        (payload) =>
          payload &&
          payload.file_path === input.file_path &&
          payload.description === input.description &&
          payload.new_content === (input.new_content ?? "")
      );
  });

  if (matchingAwaitingIndex === -1) {
    return "pending";
  }

  const resolved = runEvents
    .slice(matchingAwaitingIndex + 1)
    .some((event) => event.type === "human_input_resolved");

  return resolved ? "accepted" : "pending";
}

export const ProposeChangeToolView = ({
  item,
  runEvents = []
}: {
  item: ArtifactItem;
  runEvents?: ToolRunEvent[];
}) => {
  const input = parseToolJson<{
    file_path?: string;
    description?: string;
    new_content?: string;
    delete?: boolean;
  }>(item.toolInput);

  const payload = findProposeChangePayload(input, runEvents);
  const filePath = payload?.file_path ?? input?.file_path ?? "?";
  const description = payload?.description ?? input?.description ?? "";
  const isDelete = payload?.delete ?? input?.delete ?? false;
  const oldContent = payload?.old_content ?? "";
  const newContent = payload?.new_content ?? input?.new_content ?? "";
  const action = isDelete ? "Delete" : oldContent ? "Update" : "Create";
  const changeState = getProposeChangeState(input, runEvents);
  const stateLabel = changeState === "accepted" ? "Accepted" : "Awaiting confirmation";

  return (
    <div className='flex h-full min-h-0 flex-col p-4'>
      <div className='flex min-h-0 flex-1 flex-col gap-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='col-span-2 rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Path</p>
            <p className='break-all font-medium font-mono text-xs'>{filePath}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Action</p>
            <p className='font-medium font-mono text-xs'>{action}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>State</p>
            <p className='font-medium font-mono text-xs'>{stateLabel}</p>
          </div>
        </div>

        {description && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Description</p>
            <p className='text-xs'>{description}</p>
          </div>
        )}

        <div className='flex min-h-0 flex-1 flex-col'>
          <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Diff</p>
          {filePath !== "?" ? (
            <div className='min-h-0 flex-1 overflow-hidden rounded-md border border-border'>
              <BaseMonacoEditor
                value={isDelete ? "" : newContent}
                original={oldContent}
                language={languageFromPath(filePath)}
                diffMode
                options={{ renderSideBySide: false, readOnly: true }}
              />
            </div>
          ) : (
            <p className='text-muted-foreground text-xs'>No diff available.</p>
          )}
        </div>
      </div>
    </div>
  );
};
