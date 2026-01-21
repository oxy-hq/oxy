import { useState } from "react";
import { Copy, Check } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import { CONTEXT_TYPE_CONFIG } from "../../constants";
import HighlightedText from "./HighlightedText";
import type { ContextItem, SemanticContent } from "@/services/api/metrics";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface ContextItemDisplayProps {
  item: ContextItem;
  metricName: string;
}

export default function ContextItemDisplay({
  item,
  metricName,
}: ContextItemDisplayProps) {
  const [copied, setCopied] = useState(false);
  const config = CONTEXT_TYPE_CONFIG[item.type] || CONTEXT_TYPE_CONFIG.question;
  const isSQL = item.type === "sql" || item.type === "SQL";
  const isSemantic = item.type === "semantic";
  const content =
    typeof item.content === "string"
      ? item.content
      : JSON.stringify(item.content);

  const copyToClipboard = async () => {
    await navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (isSemantic) {
    const semanticContent = Array.isArray(item.content)
      ? (item.content as SemanticContent[])
      : [item.content as SemanticContent];

    return (
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <p
            className={cn(
              "text-xs font-medium flex items-center gap-1",
              config.color,
            )}
          >
            {config.icon} {config.label}
          </p>
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
        <div className="p-3 rounded-lg bg-gradient-to-br from-orange-500/5 to-amber-500/5 border border-orange-500/20 space-y-2">
          {semanticContent.map((semantic, idx) => (
            <div key={idx} className="space-y-1">
              {semantic.topic && (
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Topic:</span>
                  <span className="text-xs font-mono text-orange-400">
                    {semantic.topic}
                  </span>
                </div>
              )}
              {semantic.measures && semantic.measures.length > 0 && (
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-xs text-muted-foreground">
                    Measures:
                  </span>
                  {semantic.measures.map((m, i) => (
                    <span
                      key={i}
                      className={cn(
                        "px-1.5 py-0.5 rounded text-xs font-mono",
                        m.includes(metricName)
                          ? "bg-yellow-500/20 text-yellow-400 border border-yellow-500/30"
                          : "bg-blue-500/10 text-blue-400",
                      )}
                    >
                      {m}
                    </span>
                  ))}
                </div>
              )}
              {semantic.dimensions && semantic.dimensions.length > 0 && (
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-xs text-muted-foreground">
                    Dimensions:
                  </span>
                  {semantic.dimensions.map((d, i) => (
                    <span
                      key={i}
                      className={cn(
                        "px-1.5 py-0.5 rounded text-xs font-mono",
                        d === metricName
                          ? "bg-yellow-500/20 text-yellow-400 border border-yellow-500/30"
                          : "bg-green-500/10 text-green-400",
                      )}
                    >
                      {d}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between">
        <p
          className={cn(
            "text-xs font-medium flex items-center gap-1",
            config.color,
          )}
        >
          {config.icon} {config.label}
        </p>
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
      {isSQL ? (
        <SyntaxHighlighter
          language="sql"
          style={oneDark}
          customStyle={{
            margin: 0,
            borderRadius: "0.5rem",
            fontSize: "0.75rem",
          }}
          wrapLines
          className="text-xs font-mono rounded-lg bg-muted/30! border [&>code]:bg-transparent!"
          lineProps={{
            style: { wordBreak: "break-all", whiteSpace: "pre-wrap" },
          }}
        >
          {content}
        </SyntaxHighlighter>
      ) : (
        <div className="p-2 rounded-lg bg-muted/30 border text-xs">
          <HighlightedText text={content} highlight={metricName} />
        </div>
      )}
    </div>
  );
}
