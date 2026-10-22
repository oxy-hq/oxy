import { Cpu } from "lucide-react";
import type { InvocationCalls } from "@/types/apps";
import { CallWaterfallRow, IsolateStartRow } from "./CallWaterfallRow";
import { formatMs, timelineTotalMs } from "./model";

/**
 * One invocation's `ctx.*` calls as timed bars on one shared axis, zero at the
 * moment the run was decided. One grid container; the header and every row
 * are subgrids of it, so the track column is the same width on every line.
 * Each call's label sits on its own line above its bar, so the waterfall reads
 * the same in the narrow dossier as in the wide Functions section.
 */
export const CallWaterfall = ({
  trace
}: {
  trace: Pick<InvocationCalls, "duration_ms" | "init_ms" | "calls">;
}) => {
  const totalMs = timelineTotalMs(trace);
  return (
    <div
      className='grid grid-cols-[minmax(0,1fr)_auto] gap-x-3'
      data-testid='admin-app-invocation-calls-waterfall'
    >
      <div className='col-span-full grid grid-cols-subgrid border-border/60 border-b pb-1 text-[10px] text-muted-foreground uppercase tracking-[0.16em]'>
        <span className='flex justify-between'>
          <span>0</span>
          <span className='normal-case tracking-normal'>{formatMs(totalMs)}</span>
        </span>
        <span className='text-right'>Took</span>
      </div>
      {trace.init_ms != null && (
        <IsolateStartRow initMs={trace.init_ms} totalMs={totalMs} icon={Cpu} />
      )}
      {trace.calls.map((call) => (
        <CallWaterfallRow key={call.seq} call={call} totalMs={totalMs} />
      ))}
    </div>
  );
};
