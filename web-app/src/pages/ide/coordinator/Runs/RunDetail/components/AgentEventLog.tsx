import type React from "react";
import type { RunEventEntry } from "@/services/api/coordinator";

/**
 * Raw structural event log for an agent run — the unopinionated
 * fallback under the Events tab. Each row is one persisted event; the
 * payload renders as a compact JSON block so the operator can verify
 * everything the waterfall and conversation views are derived from.
 */
export const AgentEventLog: React.FC<{ events: RunEventEntry[] }> = ({ events }) => {
  if (events.length === 0) {
    return (
      <div className='px-4 py-10 text-center text-muted-foreground text-sm'>
        No structural events captured for this run.
      </div>
    );
  }

  return (
    <div className='divide-y divide-border'>
      {events.map((e) => (
        <div key={e.seq} className='flex gap-3 px-4 py-2'>
          <span className='w-12 shrink-0 text-right font-mono text-muted-foreground text-xs'>
            #{e.seq}
          </span>
          <div className='min-w-0 flex-1'>
            <p className='font-medium text-xs'>{e.event_type}</p>
            {Object.keys(e.payload).length > 0 && (
              <pre className='mt-0.5 max-h-32 overflow-y-auto whitespace-pre-wrap break-words text-muted-foreground text-xs'>
                {JSON.stringify(e.payload, null, 2)}
              </pre>
            )}
          </div>
        </div>
      ))}
    </div>
  );
};
