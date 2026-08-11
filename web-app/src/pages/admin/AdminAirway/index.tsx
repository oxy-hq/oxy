import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAirwayConfig } from "@/hooks/api/airwayConfig/useAirwayConfig";
import { DeploymentConfig } from "./DeploymentConfig";
import { SourceKindCard } from "./SourceKindCard";

/**
 * `/admin/airway` — staff console for airway's configuration. Three regions,
 * and the first two belong to a different tier than the third:
 *
 * 1. **source-kind cards** — the admission *policy* tier (`contract_policy`,
 *    `environment`), per kind, resolved on every run;
 * 2. **workspace overrides**, embedded in each card, sparse over that policy;
 * 3. **Deployment** — airway's *operational* tier, deployment-wide and
 *    installed once per worker process. Different scope, different lifetime,
 *    and crucially not live on save — see `DeploymentConfig`.
 *
 * `max_rewind`, `cursor_lag_floor`, `allow_unversioned_writes` and
 * `partition_repull_budget` appear in none of them: they have zero occurrences
 * in airway's source, so a control for one would be accepted, saved and inert
 * — the exact failure this surface exists to avoid.
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
        <h1 className='font-semibold text-xl tracking-tight'>Airway configuration</h1>
        <p className='max-w-2xl text-muted-foreground text-xs'>
          The contract policy each source kind admits pipelines under, plus the deployment-wide
          operational settings airway installs at worker startup. Tightening a kind's policy can
          halt every pipeline whose resources don't satisfy it — preview before saving.
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

      {/* Its own query, so a failure in the policy tier does not take the
          operational tier down with it (and vice versa) — two tiers, two
          tables, two independent reads. */}
      <DeploymentConfig />
    </div>
  );
}
