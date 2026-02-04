import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import type { ContextItem, SemanticContent } from "@/services/api/metrics";
import { CONTEXT_TYPE_CONFIG } from "../../constants";
import HighlightedText from "./HighlightedText";

interface ContextItemDisplayProps {
  item: ContextItem;
  metricName: string;
}

export default function ContextItemDisplay({ item, metricName }: ContextItemDisplayProps) {
  const [copied, setCopied] = useState(false);
  const config = CONTEXT_TYPE_CONFIG[item.type] || CONTEXT_TYPE_CONFIG.question;
  const isSQL = item.type === "sql" || item.type === "SQL";
  const isSemantic = item.type === "semantic";
  const content = typeof item.content === "string" ? item.content : JSON.stringify(item.content);

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
      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <p className={cn("flex items-center gap-1 font-medium text-xs", config.color)}>
            {config.icon} {config.label}
          </p>
          <Button variant='ghost' size='sm' className='h-6 px-2' onClick={copyToClipboard}>
            {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
          </Button>
        </div>
        <div className='space-y-2 rounded-lg border border-orange-500/20 bg-gradient-to-br from-orange-500/5 to-amber-500/5 p-3'>
          {semanticContent.map((semantic, idx) => (
            <div key={idx} className='space-y-1'>
              {semantic.topic && (
                <div className='flex items-center gap-2'>
                  <span className='text-muted-foreground text-xs'>Topic:</span>
                  <span className='font-mono text-orange-400 text-xs'>{semantic.topic}</span>
                </div>
              )}
              {semantic.measures && semantic.measures.length > 0 && (
                <div className='flex flex-wrap items-center gap-2'>
                  <span className='text-muted-foreground text-xs'>Measures:</span>
                  {semantic.measures.map((m, i) => (
                    <span
                      key={i}
                      className={cn(
                        "rounded px-1.5 py-0.5 font-mono text-xs",
                        m.includes(metricName)
                          ? "border border-yellow-500/30 bg-yellow-500/20 text-yellow-400"
                          : "bg-blue-500/10 text-blue-400"
                      )}
                    >
                      {m}
                    </span>
                  ))}
                </div>
              )}
              {semantic.dimensions && semantic.dimensions.length > 0 && (
                <div className='flex flex-wrap items-center gap-2'>
                  <span className='text-muted-foreground text-xs'>Dimensions:</span>
                  {semantic.dimensions.map((d, i) => (
                    <span
                      key={i}
                      className={cn(
                        "rounded px-1.5 py-0.5 font-mono text-xs",
                        d === metricName
                          ? "border border-yellow-500/30 bg-yellow-500/20 text-yellow-400"
                          : "bg-green-500/10 text-green-400"
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
    <div className='space-y-1'>
      <div className='flex items-center justify-between'>
        <p className={cn("flex items-center gap-1 font-medium text-xs", config.color)}>
          {config.icon} {config.label}
        </p>
        <Button variant='ghost' size='sm' className='h-6 px-2' onClick={copyToClipboard}>
          {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
        </Button>
      </div>
      {isSQL ? (
        <SyntaxHighlighter
          language='sql'
          style={oneDark}
          customStyle={{
            margin: 0,
            borderRadius: "0.5rem",
            fontSize: "0.75rem"
          }}
          wrapLines
          className='rounded-lg border bg-muted/30! font-mono text-xs [&>code]:bg-transparent!'
          lineProps={{
            style: { wordBreak: "break-all", whiteSpace: "pre-wrap" }
          }}
        >
          {content}
        </SyntaxHighlighter>
      ) : (
        <div className='rounded-lg border bg-muted/30 p-2 text-xs'>
          <HighlightedText text={content} highlight={metricName} />
        </div>
      )}
    </div>
  );
}
