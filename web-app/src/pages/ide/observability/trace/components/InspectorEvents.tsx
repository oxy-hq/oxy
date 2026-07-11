import type { TimelineSpan } from "@/services/api/traces";
import { AttributeCard } from "./AttributeCard";

interface InspectorEventsProps {
  span: TimelineSpan;
}

// Internal/metadata attributes not worth surfacing per event.
const HIDDEN_EVENT_ATTRS = new Set([
  "is_visible",
  "code.filepath",
  "code.lineno",
  "code.namespace",
  "level",
  "name",
  "target",
  "status"
]);

export function InspectorEvents({ span }: InspectorEventsProps) {
  if (span.events.length === 0) {
    return <p className='text-muted-foreground text-xs'>No events recorded on this span.</p>;
  }

  return (
    <div className='space-y-4'>
      {span.events.map((event, index) => {
        const attrs = Object.entries(event.attributes).filter(
          ([key]) => !HIDDEN_EVENT_ATTRS.has(key)
        );
        return (
          <div key={`${event.name}-${index}`} className='space-y-2'>
            <div className='font-mono text-muted-foreground text-xs uppercase tracking-wide'>
              {event.attributes.name || event.name || `event ${index + 1}`}
            </div>
            {attrs.length === 0 ? (
              <p className='text-muted-foreground text-xs'>No attributes.</p>
            ) : (
              attrs.map(([key, value]) => <AttributeCard key={key} name={key} value={value} />)
            )}
          </div>
        );
      })}
    </div>
  );
}
