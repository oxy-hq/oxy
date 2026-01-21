import { useState } from "react";
import { Copy, Check } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { deepParseJson } from "../../../trace/components/utils";

interface OutputDisplayProps {
  value: string;
  label: string;
}

export default function DataDisplay({ value, label }: OutputDisplayProps) {
  const [copied, setCopied] = useState(false);

  const copyToClipboard = async () => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  let parsedValue: string | null = null;
  let isJson = false;
  try {
    const parsed = JSON.parse(value);
    const deepParsed = deepParseJson(parsed);
    parsedValue = JSON.stringify(deepParsed, null, 2);
    isJson = true;
  } catch {
    // Not JSON, use as-is
  }

  const displayValue = !isJson ? value : parsedValue!;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-xs text-muted-foreground font-medium">
          {label}
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 px-2"
          onClick={copyToClipboard}
        >
          {copied ? (
            <Check className="w-3 h-3" />
          ) : (
            <Copy className="w-3 h-3" />
          )}
        </Button>
      </div>
      {isJson ? (
        <SyntaxHighlighter
          language="json"
          style={oneDark}
          customStyle={{
            margin: 0,
            borderRadius: "0.5rem",
            fontSize: "0.75rem",
          }}
          className="text-xs font-mono rounded-lg bg-muted/30! border [&>code]:bg-transparent!"
          wrapLines
          lineProps={{
            style: { wordBreak: "break-all", whiteSpace: "pre-wrap" },
          }}
        >
          {displayValue}
        </SyntaxHighlighter>
      ) : (
        <pre className="overflow-x-auto whitespace-pre-wrap max-h-48 overflow-y-auto p-2 rounded-lg bg-muted/30 border text-xs">
          {displayValue}
        </pre>
      )}
    </div>
  );
}
