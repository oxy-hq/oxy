import { Panel, PanelHeader } from "@/components/ui/panel";
import { Badge } from "@/components/ui/shadcn/badge";
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
    <Panel>
      <PanelHeader
        title={
          <div className='flex items-center gap-2'>
            <SpanIcon spanName={span.spanName} className='h-4 w-4 shrink-0 text-muted-foreground' />
            <h2 className='truncate font-semibold text-sm' title={span.spanName}>
              {formatSpanLabel(span.spanName)}
            </h2>
          </div>
        }
        subtitle={
          <div className='mt-1.5 flex flex-wrap items-center gap-2'>
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
        }
        onClose={onClose}
      />

      {/* Content */}
      {hasVisibleEvents ? (
        <Tabs
          defaultValue={visibleEvents[0]?.attributes.name || "0"}
          className='flex flex-1 flex-col overflow-hidden'
        >
          <div className='border-b'>
            <TabsList className='flex h-auto flex-wrap items-start justify-start gap-2 rounded-none border-none bg-transparent px-4 py-2'>
              {visibleEvents.map((event, index) => (
                <TabsTrigger
                  key={index}
                  value={event.attributes.name || String(index)}
                  className='rounded-md px-3 py-1.5 text-xs transition-colors hover:text-foreground data-[state=active]:bg-accent! data-[state=active]:text-foreground data-[state=inactive]:text-muted-foreground'
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
    </Panel>
  );
}
