import type { CustomApp } from "@/types/apps";
import { StatusDot } from "./AppsTable/components/StatusDot";
import { fleetStats } from "./AppsTable/useAppsTable";

/**
 * The cockpit's top readout: a slim, single-row rollup of the whole registry —
 * total apps, live vs draft (with the same LED language the rows use), distinct
 * orgs, and a source breakdown pinned right. Reads left→right as "how big is
 * the fleet, how much of it is shipped, spread across how many tenants, built
 * which ways." Derived client-side from the already-loaded list, so it costs
 * nothing extra and stays exact as filters change upstream.
 */
export const FleetStrip = ({ apps }: { apps: CustomApp[] }) => {
  const s = fleetStats(apps);
  return (
    <div className='flex flex-wrap items-center gap-x-5 gap-y-1.5 border-b bg-muted/20 px-4 py-2 text-xs'>
      <Stat value={s.total} label='apps' />
      <Stat value={s.live} label='live' led='live' />
      <Stat value={s.draft} label='draft' led='draft' />
      <Stat value={s.orgs} label={s.orgs === 1 ? "org" : "orgs"} />

      <div className='ml-auto flex items-center gap-2.5 font-mono text-[11px] text-muted-foreground'>
        {s.bySource.s3 > 0 && <SourceChip label='S3' n={s.bySource.s3} />}
        {s.bySource.v0 > 0 && <SourceChip label='v0' n={s.bySource.v0} />}
        {s.bySource.local > 0 && <SourceChip label='local' n={s.bySource.local} />}
      </div>
    </div>
  );
};

const Stat = ({ value, label, led }: { value: number; label: string; led?: "live" | "draft" }) => (
  <span className='flex items-center gap-1.5'>
    {led && <StatusDot isLive={led === "live"} decorative />}
    <span className='font-semibold text-foreground text-xs tabular-nums'>{value}</span>
    <span className='text-muted-foreground'>{label}</span>
  </span>
);

const SourceChip = ({ label, n }: { label: string; n: number }) => (
  <span className='flex items-center gap-1'>
    <span className='text-muted-foreground/60'>{label}</span>
    <span className='text-foreground tabular-nums'>{n}</span>
  </span>
);
