import { useInvocationCalls } from "@/hooks/api/customApps/useAppFunctions";
import { AdminAsync } from "@/pages/admin/components/AdminAsync";
import { CallWaterfall } from "./CallWaterfall";
import { incompleteNote, unrecordedText } from "./model";

/**
 * What one invocation did, call by call: the waterfall of its `ctx.*` calls,
 * fetched when this mounts (a row was opened). A failed fetch is the
 * AdminAsync error — never "made no calls" — and an invocation with no trace
 * says why, by the server's `reason`.
 */
export const InvocationCalls = ({
  appId,
  invocationId
}: {
  appId: string;
  invocationId: string;
}) => {
  const query = useInvocationCalls(appId, invocationId, true);
  return (
    <div className='flex flex-col gap-1.5' data-testid='admin-app-invocation-calls'>
      <AdminAsync query={query} noun='the call trace' rows={2}>
        {(trace) => {
          if (!trace.recorded) {
            return (
              <p
                className='text-muted-foreground text-xs'
                data-testid='admin-app-invocation-calls-unrecorded'
                data-reason={trace.reason ?? "not_recorded"}
              >
                {unrecordedText(trace.reason, trace.retention_days)}
              </p>
            );
          }
          if (trace.total_calls === 0) {
            return (
              <>
                {trace.init_ms != null && <CallWaterfall trace={trace} />}
                <p
                  className='text-muted-foreground text-xs'
                  data-testid='admin-app-invocation-calls-empty'
                >
                  Made no ctx calls.
                </p>
              </>
            );
          }
          const note = incompleteNote(trace);
          return (
            <>
              <CallWaterfall trace={trace} />
              {note && (
                <p
                  className='text-muted-foreground text-xs'
                  data-testid='admin-app-invocation-calls-truncated'
                >
                  {note}
                </p>
              )}
            </>
          );
        }}
      </AdminAsync>
    </div>
  );
};
