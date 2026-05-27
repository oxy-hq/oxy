import { Brain, Coins, DollarSign, Hash } from "lucide-react";
import type React from "react";
import type { LlmUsage, RunEventEntry } from "@/services/api/coordinator";
import { formatTokens, formatUsd } from "../../../components/utils";

/**
 * Per-run token + cost summary for agent runs — the load-bearing part of
 * the Agent debugging unit ("trace": tokens + cost/run + model). Backend
 * computes this from the persisted `llm_start`/`llm_end` events; no
 * extension table or schema change needed.
 *
 * Renders an explicit empty state when `usage` is null or undefined so
 * agent runs without captured `llm_end` events surface the missing-data
 * reason instead of silently dropping the whole card.
 */
export const LlmUsageCard: React.FC<{
  usage: LlmUsage | null | undefined;
  /** Filtered structural event log for this run; used by the empty
   *  state to surface what *was* recorded when usage is missing — turns
   *  "no LLM events" from a dead end into a diagnostic. */
  events?: RunEventEntry[];
}> = ({ usage, events }) => {
  if (!usage) {
    const eventTypes =
      events && events.length > 0
        ? Array.from(new Set(events.map((e) => e.event_type))).sort()
        : [];
    return (
      <div className='border-border border-b bg-card'>
        <div className='flex items-center gap-2 px-4 py-2'>
          <Brain className='h-4 w-4 text-muted-foreground' />
          <h3 className='font-semibold text-sm'>LLM usage</h3>
          <span className='text-muted-foreground text-xs'>no llm_end events recorded</span>
        </div>
        <div className='space-y-1 px-4 pb-3 text-muted-foreground text-xs'>
          <p>
            <code>usage_report_for_run</code> returned no rows for this run — its event log holds 0
            events of type <code>llm_end</code>.
          </p>
          {events !== undefined &&
            (events.length === 0 ? (
              <p>
                No structural events of any kind were persisted. The agent likely errored before any
                FSM transition, or the bridge dropped them. Check the server log.
              </p>
            ) : (
              <p>
                Other events on this run ({events.length} total):{" "}
                <code>{eventTypes.join(", ")}</code>. See the Events tab for full payloads.
              </p>
            ))}
        </div>
      </div>
    );
  }

  const totalInput = usage.input_tokens + usage.cache_creation_input_tokens;
  const cacheHitPct =
    usage.cache_read_input_tokens > 0
      ? Math.round(
          (usage.cache_read_input_tokens / (usage.cache_read_input_tokens + totalInput || 1)) * 100
        )
      : null;

  const hasCost = usage.cost_usd !== null && usage.cost_usd !== undefined;
  const modelsLabel = usage.models.join(", ") || "—";
  // Distinguish "no model captured on the events" (write-time bug) from
  // "model captured but not in the pricing table" (just needs an update
  // to crates/agentic/llm/src/pricing.rs).
  const costHint = hasCost
    ? "tokens × per-million rates"
    : usage.models.length === 0
      ? "No model recorded on llm_end events"
      : `No pricing for ${modelsLabel}`;

  return (
    <div className='border-border border-b bg-card'>
      <div className='flex items-center gap-2 px-4 py-2'>
        <Brain className='h-4 w-4 text-primary' />
        <h3 className='font-semibold text-sm'>LLM usage</h3>
        <span className='text-muted-foreground text-xs'>
          {usage.call_count} call{usage.call_count === 1 ? "" : "s"} · {modelsLabel}
        </span>
      </div>
      <div className='grid grid-cols-2 gap-3 px-4 pb-3 md:grid-cols-4'>
        <Metric
          icon={Hash}
          label='Input'
          value={formatTokens(usage.input_tokens)}
          hint={
            usage.cache_creation_input_tokens > 0
              ? `+${formatTokens(usage.cache_creation_input_tokens)} cache writes`
              : undefined
          }
        />
        <Metric icon={Hash} label='Output' value={formatTokens(usage.output_tokens)} />
        <Metric
          icon={Coins}
          label='Cache reads'
          value={formatTokens(usage.cache_read_input_tokens)}
          hint={cacheHitPct !== null ? `${cacheHitPct}% of inputs` : undefined}
        />
        <Metric
          icon={DollarSign}
          label='Cost (est.)'
          value={hasCost ? `~${formatUsd(usage.cost_usd as number)}` : "—"}
          hint={costHint}
        />
      </div>
    </div>
  );
};

const Metric: React.FC<{
  icon: React.ElementType;
  label: string;
  value: string;
  hint?: string;
}> = ({ icon: Icon, label, value, hint }) => (
  <div className='flex items-start gap-2'>
    <Icon className='mt-0.5 h-4 w-4 shrink-0 text-muted-foreground' />
    <div className='min-w-0'>
      <p className='text-muted-foreground text-xs uppercase tracking-wide'>{label}</p>
      <p className='font-semibold text-sm tabular-nums'>{value}</p>
      {hint && <p className='text-muted-foreground text-xs'>{hint}</p>}
    </div>
  </div>
);
