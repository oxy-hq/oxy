import Markdown from "@/components/Markdown";
import BaseMonacoEditor from "@/components/MonacoEditor/BaseMonacoEditor";
import { Button } from "@/components/ui/shadcn/button";

export interface ProposeChangePayload {
  type: "propose_change";
  file_path: string;
  old_content: string;
  new_content: string;
  description: string;
  delete?: boolean;
}

export function parseProposeChange(prompt: string): ProposeChangePayload | null {
  try {
    const parsed = JSON.parse(prompt);
    if (parsed?.type === "propose_change") return parsed as ProposeChangePayload;
  } catch {
    // not JSON
  }
  return null;
}

export function languageFromPath(filePath: string): string {
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

interface ProposeChangeDiffProps {
  payload: ProposeChangePayload;
  suggestions: string[];
  onAnswer: (text: string) => void;
  isAnswering: boolean;
}

const ProposeChangeDiff = ({
  payload,
  suggestions,
  onAnswer,
  isAnswering
}: ProposeChangeDiffProps) => {
  return (
    <div className='space-y-3'>
      <div>
        <p className='mb-0.5 text-muted-foreground text-xs'>{payload.file_path}</p>
        <Markdown>{payload.description}</Markdown>
      </div>
      {payload.delete ? (
        <div className='overflow-hidden rounded-md border border-destructive/50 bg-destructive/5'>
          <div className='border-destructive/20 border-b px-3 py-2'>
            <span className='font-medium text-destructive text-xs'>File will be deleted</span>
          </div>
          <div style={{ height: 200 }}>
            <BaseMonacoEditor
              value=''
              original={payload.old_content}
              language={languageFromPath(payload.file_path)}
              diffMode
              options={{ renderSideBySide: false, readOnly: true }}
            />
          </div>
        </div>
      ) : (
        <div className='overflow-hidden rounded-md border border-border' style={{ height: 360 }}>
          <BaseMonacoEditor
            value={payload.new_content}
            original={payload.old_content}
            language={languageFromPath(payload.file_path)}
            diffMode
            options={{ renderSideBySide: false, readOnly: true }}
          />
        </div>
      )}
      {suggestions.length > 0 && (
        <div className='flex flex-wrap gap-2'>
          {suggestions.map((s) => (
            <Button
              key={s}
              variant={s === "Accept" ? "default" : "outline"}
              size='sm'
              onClick={() => onAnswer(s)}
              disabled={isAnswering}
            >
              {s}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
};

export default ProposeChangeDiff;
