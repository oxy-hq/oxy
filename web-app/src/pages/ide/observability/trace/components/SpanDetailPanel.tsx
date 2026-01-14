import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Badge } from "@/components/ui/shadcn/badge";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/shadcn/tabs";
import type { TimelineSpan } from "@/services/api/traces";
import { formatDuration, formatSpanLabel, SpanIcon } from "../../utils/index";
import { AttributeCard } from "./AttributeCard";

interface SpanDetailPanelProps {
  span: TimelineSpan;
  onClose: () => void;
}

export function SpanDetailPanel({ span, onClose }: SpanDetailPanelProps) {
  // Filter events that have is_visible = true attribute
  const visibleEvents = span.events.filter(
    (event) => event.attributes["is_visible"] === "true",
  );

  // Attributes to hide from display (internal/metadata)
  const hiddenAttributes = new Set([
    "is_visible",
    "code.filepath",
    "code.lineno",
    "code.namespace",
    "level",
    "name",
    "target",
    "status",
  ]);

  // Get filtered attributes for an event
  const getEventAttributes = (event: (typeof visibleEvents)[0]) => {
    return Object.entries(event.attributes).filter(
      ([key]) => !hiddenAttributes.has(key),
    );
  };

  const hasVisibleEvents = visibleEvents.length > 0;

  return (
    <div className="flex flex-col h-full border-l">
      {/* Header */}
      <div className="px-4 py-3 border-b flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2">
            <SpanIcon
              spanName={span.spanName}
              className="h-5 w-5 flex-shrink-0 text-muted-foreground"
            />
            <h2
              className="font-semibold text-base truncate"
              title={span.spanName}
            >
              {formatSpanLabel(span.spanName)}
            </h2>
          </div>
          <div className="flex items-center gap-2 flex-wrap">
            <Badge
              variant={
                span.statusCode === "Error" ? "destructive" : "secondary"
              }
              className="text-xs"
            >
              {span.statusCode || "Unset"}
            </Badge>
            <Badge variant="outline" className="text-xs">
              {formatDuration(span.durationMs)}
            </Badge>
            {span.spanKind && (
              <Badge variant="outline" className="text-xs">
                {span.spanKind}
              </Badge>
            )}
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 flex-shrink-0"
          onClick={onClose}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Content */}
      {hasVisibleEvents ? (
        <Tabs
          defaultValue={visibleEvents[0]?.attributes["name"] || "0"}
          className="flex-1 flex flex-col overflow-hidden"
        >
          <div className="border-b">
            <TabsList className="px-4 py-2 rounded-none justify-start items-start h-auto gap-2 bg-transparent flex flex-wrap">
              {visibleEvents.map((event, index) => (
                <TabsTrigger
                  key={index}
                  value={event.attributes["name"] || String(index)}
                  className="text-xs px-3 py-1.5 rounded-md data-[state=active]:bg-accent data-[state=active]:text-foreground data-[state=inactive]:text-muted-foreground hover:text-foreground transition-colors"
                >
                  {event.attributes["name"] || `Event ${index + 1}`}
                </TabsTrigger>
              ))}
            </TabsList>
          </div>

          <div className="flex-1 overflow-auto customScrollbar scrollbar-gutter-auto">
            {visibleEvents.map((event, index) => (
              <TabsContent
                key={index}
                value={event.attributes["name"] || String(index)}
                className="p-4 mt-0 space-y-4"
              >
                {getEventAttributes(event).map(([key, value]) => (
                  <AttributeCard key={key} name={key} value={value} />
                ))}
              </TabsContent>
            ))}
          </div>
        </Tabs>
      ) : (
        <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
          No data collected
        </div>
      )}
    </div>
  );
}
