import { useState } from "react";
import { Copy, Check, Maximize2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/shadcn/toggle-group";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import { deepParseJson } from "./utils";

interface AttributeCardProps {
  name: string;
  value: string;
}

export function AttributeCard({ name, value }: AttributeCardProps) {
  const [copied, setCopied] = useState(false);
  const [showRaw, setShowRaw] = useState(false);
  const [isExpanded, setIsExpanded] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // Try to parse JSON for pretty display, resolving nested JSON strings
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

  const displayValue = showRaw || !isJson ? value : parsedValue!;

  return (
    <>
      <div className="rounded-lg border overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-3 py-2 bg-muted/50 border-b">
          <span className="text-xs font-semibold">{name}</span>
          <div className="flex items-center gap-1">
            {isJson && (
              <ToggleGroup
                type="single"
                value={showRaw ? "raw" : "json"}
                onValueChange={(val) => val && setShowRaw(val === "raw")}
                size="sm"
                className="h-7"
              >
                <ToggleGroupItem value="json" className="text-xs px-2 h-6">
                  JSON
                </ToggleGroupItem>
                <ToggleGroupItem value="raw" className="text-xs px-2 h-6">
                  Raw
                </ToggleGroupItem>
              </ToggleGroup>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={handleCopy}
              title="Copy"
            >
              {copied ? (
                <Check className="h-3.5 w-3.5 text-green-500" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => setIsExpanded(true)}
              title="Expand"
            >
              <Maximize2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
        {/* Content */}
        <div className="overflow-auto max-h-80 customScrollbar scrollbar-gutter-auto">
          {!showRaw && isJson ? (
            <SyntaxHighlighter
              language="json"
              style={oneDark}
              className="p-4! m-0! bg-zinc-900! [&>code]:bg-transparent! text-sm font-mono"
              showLineNumbers
            >
              {displayValue}
            </SyntaxHighlighter>
          ) : (
            <pre className="text-sm p-3 whitespace-pre-wrap break-all bg-zinc-900!">
              {displayValue}
            </pre>
          )}
        </div>
      </div>

      {/* Expanded Dialog */}
      <Dialog open={isExpanded} onOpenChange={setIsExpanded}>
        <DialogContent
          showCloseButton={false}
          className="max-w-4xl max-h-[90vh] flex flex-col"
          onPointerDownOutside={() => setIsExpanded(false)}
        >
          <DialogHeader className="flex-shrink-0">
            <DialogTitle className="flex items-center justify-between">
              <span>{name}</span>
              <div className="flex items-center gap-2">
                {isJson && (
                  <ToggleGroup
                    type="single"
                    value={showRaw ? "raw" : "json"}
                    onValueChange={(val) => val && setShowRaw(val === "raw")}
                    size="sm"
                  >
                    <ToggleGroupItem value="json" className="text-xs px-2">
                      JSON
                    </ToggleGroupItem>
                    <ToggleGroupItem value="raw" className="text-xs px-2">
                      Raw
                    </ToggleGroupItem>
                  </ToggleGroup>
                )}
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={handleCopy}
                >
                  {copied ? (
                    <Check className="h-4 w-4 text-green-500" />
                  ) : (
                    <Copy className="h-4 w-4" />
                  )}
                </Button>
              </div>
            </DialogTitle>
          </DialogHeader>
          <div className="overflow-auto max-h-80 customScrollbar scrollbar-gutter-auto">
            {!showRaw && isJson ? (
              <SyntaxHighlighter
                language="json"
                style={oneDark}
                className="p-4! m-0! bg-zinc-900! [&>code]:bg-transparent! text-sm font-mono"
                showLineNumbers
              >
                {displayValue}
              </SyntaxHighlighter>
            ) : (
              <pre className="text-sm p-3 whitespace-pre-wrap break-all bg-zinc-900!">
                {displayValue}
              </pre>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
