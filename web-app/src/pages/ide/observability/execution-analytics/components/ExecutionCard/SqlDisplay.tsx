import { useState } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { Copy, Check } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface SqlDisplayProps {
  sql: string;
  label?: string;
  isPreview?: boolean;
}

export default function SqlDisplay({
  sql,
  label = "SQL Query",
  isPreview = false,
}: SqlDisplayProps) {
  const [copied, setCopied] = useState(false);

  const copyToClipboard = async () => {
    await navigator.clipboard.writeText(sql);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (isPreview) {
    return (
      <div className="pt-1 pb-1">
        <SyntaxHighlighter
          language="sql"
          style={oneDark}
          customStyle={{
            margin: "0",
            borderRadius: "0.5rem",
            fontSize: "0.75rem",
          }}
          className="text-xs font-mono"
          wrapLines
          lineProps={{
            style: { wordBreak: "break-all", whiteSpace: "pre-wrap" },
          }}
        >
          {sql.slice(0, 100) + (sql.length > 100 ? "..." : "")}
        </SyntaxHighlighter>
      </div>
    );
  }

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
      <SyntaxHighlighter
        language="sql"
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
        {sql}
      </SyntaxHighlighter>
    </div>
  );
}
