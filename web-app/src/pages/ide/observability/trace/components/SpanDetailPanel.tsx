import { X } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import type { TimelineSpan } from "@/services/api/traces";
import { formatDuration, formatSpanLabel, SpanIcon } from "../../utils/index";
import { AttributeCard } from "./AttributeCard";

interface SpanDetailPanelProps {
  span: TimelineSpan;
  onClose: () => void;
}

export function SpanDetailPanel({ span, onClose }: SpanDetailPanelProps) {
  // Filter events that have is_visible = true attribute
  const visibleEvents = span.events.filter((event) => event.attributes.is_visible === "true");

  // Attributes to hide from display (internal/metadata)
  const hiddenAttributes = new Set([
    "is_visible",
    "code.filepath",
    "code.lineno",
    "code.namespace",
    "level",
    "name",
    "target",
    "status"
  ]);

  // Get filtered attributes for an event
  const getEventAttributes = (event: (typeof visibleEvents)[0]) => {
    return Object.entries(event.attributes).filter(([key]) => !hiddenAttributes.has(key));
  };

  const hasVisibleEvents = visibleEvents.length > 0;

  return (
    <div className='flex h-full flex-col border-l'>
      {/* Header */}
      <div className='flex items-start justify-between gap-3 border-b px-4 py-3'>
        <div className='min-w-0 flex-1'>
          <div className='mb-2 flex items-center gap-2'>
            <SpanIcon
              spanName={span.spanName}
              className='h-5 w-5 flex-shrink-0 text-muted-foreground'
            />
            <h2 className='truncate font-semibold text-base' title={span.spanName}>
              {formatSpanLabel(span.spanName)}
            </h2>
          </div>
          <div className='flex flex-wrap items-center gap-2'>
            <Badge
              variant={span.statusCode === "Error" ? "destructive" : "secondary"}
              className='text-xs'
            >
              {span.statusCode || "Unset"}
            </Badge>
            <Badge variant='outline' className='text-xs'>
              {formatDuration(span.durationMs)}
            </Badge>
            {span.spanKind && (
              <Badge variant='outline' className='text-xs'>
                {span.spanKind}
              </Badge>
            )}
          </div>
        </div>
        <Button variant='ghost' size='icon' className='h-8 w-8 flex-shrink-0' onClick={onClose}>
          <X className='h-4 w-4' />
        </Button>
      </div>

      {/* Content */}
      {hasVisibleEvents ? (
        <Tabs
          defaultValue={visibleEvents[0]?.attributes.name || "0"}
          className='flex flex-1 flex-col overflow-hidden'
        >
          <div className='border-b'>
            <TabsList className='flex h-auto flex-wrap items-start justify-start gap-2 rounded-none bg-transparent px-4 py-2'>
              {visibleEvents.map((event, index) => (
                <TabsTrigger
                  key={index}
                  value={event.attributes.name || String(index)}
                  className='rounded-md px-3 py-1.5 text-xs transition-colors hover:text-foreground data-[state=active]:bg-accent data-[state=active]:text-foreground data-[state=inactive]:text-muted-foreground'
                >
                  {event.attributes.name || `Event ${index + 1}`}
                </TabsTrigger>
              ))}
            </TabsList>
          </div>

          <div className='customScrollbar scrollbar-gutter-auto flex-1 overflow-auto'>
            {visibleEvents.map((event, index) => (
              <TabsContent
                key={index}
                value={event.attributes.name || String(index)}
                className='mt-0 space-y-4 p-4'
              >
                {getEventAttributes(event).map(([key, value]) => (
                  <AttributeCard key={key} name={key} value={value} />
                ))}
              </TabsContent>
            ))}
          </div>
        </Tabs>
      ) : (
        <div className='flex flex-1 items-center justify-center text-muted-foreground text-sm'>
          No data collected
        </div>
      )}
    </div>
  );
}
