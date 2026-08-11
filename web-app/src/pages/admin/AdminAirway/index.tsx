import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAirwayConfig } from "@/hooks/api/airwayConfig/useAirwayConfig";
import { SourceKindCard } from "./SourceKindCard";

/**
 * `/admin/airway` — staff console for airway's per-source-kind
 * admission policy. Two regions only: source-kind cards (each embedding its
 * own workspace-overrides table) — no "Deployment" region. `max_rewind`,
 * `cursor_lag_floor`, and per-resource restatement windows are stage 4,
 * deliberately: the code that honours them doesn't exist yet, and a knob
 * that does nothing is the exact failure this surface exists to avoid.
 *
 * Tightening a kind's `contract_policy` can silently halt every pipeline
 * whose resources don't satisfy it, so the preview is the guardrail — see
 * `SourceKindCard` for how selects never save implicitly and `PolicyPreview`
 * for how a preview is invalidated the moment a select changes.
 */
export default function AdminAirway() {
  const { data, isLoading, isError } = useAirwayConfig();

  return (
    <div className='mx-auto max-w-5xl space-y-4 p-6 pb-20 lg:px-10 lg:py-8'>
      <header className='space-y-1'>
        <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
          Admin · Airway
        </p>
        <h1 className='font-semibold text-xl tracking-tight'>Admission policy</h1>
        <p className='max-w-2xl text-muted-foreground text-xs'>
          The contract policy each source kind admits pipelines under. Tightening a kind's policy
          can halt every pipeline whose resources don't satisfy it — preview before saving.
        </p>
      </header>

      {isLoading ? (
        <div className='space-y-4' data-testid='admin-airway-loading'>
          <Skeleton className='h-48 w-full' />
          <Skeleton className='h-48 w-full' />
        </div>
      ) : isError ? (
        <div
          className='rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-destructive text-xs'
          data-testid='admin-airway-error'
        >
          Failed to load airway admission config.
        </div>
      ) : !data || data.kinds.length === 0 ? (
        <div
          className='rounded-lg border border-border/60 border-dashed bg-muted/30 p-6 text-center text-muted-foreground text-xs'
          data-testid='admin-airway-empty'
        >
          No known source kinds.
        </div>
      ) : (
        <div className='space-y-4' data-testid='admin-airway-source-kind-list'>
          {data.kinds.map((kind) => (
            <SourceKindCard key={kind.source_kind} kind={kind} />
          ))}
        </div>
      )}
    </div>
  );
}
