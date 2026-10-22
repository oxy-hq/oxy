import { InvocationCalls } from "../../../CallWaterfall/InvocationCalls";
import { invocationIdOf, type OxyRequestEntry } from "../../useOxyRequestLog";
import { getHeader } from "../format";

/**
 * The server's side of one captured function call: the `ctx.*` calls of the
 * invocation its answer named (`x-oxy-invocation-id`). Only the published
 * frame is same-origin, so only a production call ever lands here.
 */
export const ServerSide = ({ appId, entry }: { appId: string; entry: OxyRequestEntry }) => {
  if (entry.status == null && !entry.error) {
    return <p className='text-muted-foreground text-xs'>Waiting for the answer…</p>;
  }
  const invocationId = invocationIdOf(entry);
  if (!invocationId) {
    return (
      <p className='text-muted-foreground text-xs' data-testid='admin-app-debug-server-side-none'>
        No invocation ran for this call: a result-cache hit or a refusal before it started.
      </p>
    );
  }
  // An idempotent retry answers with the invocation it replays, which ran
  // earlier — the calls below may be that one's, not this request's.
  const keyed = getHeader(entry.reqHeaders, "idempotency-key") != null;
  return (
    <div className='flex flex-col gap-2'>
      {keyed && (
        <p
          className='text-muted-foreground text-xs'
          data-testid='admin-app-debug-server-side-replay'
        >
          Sent with an Idempotency-Key: a retry is answered by the earlier invocation it replays, so
          these may be that call&rsquo;s.
        </p>
      )}
      <InvocationCalls appId={appId} invocationId={invocationId} />
    </div>
  );
};
