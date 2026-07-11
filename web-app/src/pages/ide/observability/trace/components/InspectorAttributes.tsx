import { useMemo, useState } from "react";
import { Input } from "@/components/ui/shadcn/input";
import { cn } from "@/libs/shadcn/utils";
import type { TimelineSpan } from "@/services/api/traces";
import { isSecretAttribute } from "./spanInspect";

interface InspectorAttributesProps {
  span: TimelineSpan;
}

/**
 * Every span attribute (resource + span + oxy.*), with a filter box and secret
 * redaction — not just the `is_visible` events the old panel exposed.
 */
export function InspectorAttributes({ span }: InspectorAttributesProps) {
  const [filter, setFilter] = useState("");

  const entries = useMemo(
    () => Object.entries(span.attributes).sort(([a], [b]) => a.localeCompare(b)),
    [span.attributes]
  );

  const query = filter.trim().toLowerCase();
  const visible = query ? entries.filter(([key]) => key.toLowerCase().includes(query)) : entries;

  return (
    <div className='space-y-3'>
      <Input
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder='filter attributes…'
        className='h-8 font-mono text-xs'
      />
      {visible.length === 0 ? (
        <p className='text-muted-foreground text-xs'>No attributes match “{filter}”.</p>
      ) : (
        <div className='divide-y divide-border'>
          {visible.map(([key, value]) => {
            const redact = isSecretAttribute(key, value);
            return (
              <div key={key} className='grid grid-cols-[9.5rem_1fr] gap-2 py-1.5 font-mono text-xs'>
                <span className='break-all text-muted-foreground'>{key}</span>
                <span className={cn("break-all", redact && "text-muted-foreground italic")}>
                  {redact ? "⋯ redacted" : value}
                </span>
              </div>
            );
          })}
        </div>
      )}
      <p className='text-muted-foreground text-xs'>
        {entries.length} span attributes — resource + span + oxy.*
      </p>
    </div>
  );
}
