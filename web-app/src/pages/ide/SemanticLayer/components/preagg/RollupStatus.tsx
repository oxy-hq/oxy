import dayjs from "dayjs";
import { Circle, Cloud, CloudOff, Zap, ZapOff } from "lucide-react";
import { formatDate, timeAgo } from "@/libs/utils/date";
import type { PreaggRollupStatus } from "@/services/api/semantic";

type CacheFacts = Pick<PreaggRollupStatus, "is_built" | "has_parquet" | "empty_since">;

/**
 * Four states, not two.
 *
 * A rollup is *built* when some node in the fleet has built it — that is what
 * decides whether queries skip the warehouse, because a node without the file
 * reads the same object from the blob store. Whether THIS node also holds the
 * Parquet is a locality detail that only changes how fast the read is.
 *
 * Collapsing the two showed a rollup another node built as "Not cached" beside
 * a real Built timestamp — the row disagreeing with itself. "Not built" stays
 * a status rather than an error: that query still runs, against the warehouse.
 */
const state = (rollup: CacheFacts, blobReads: boolean) => {
  if (!rollup.is_built) {
    // "Empty" and "Not built" are both un-built rows, and they are not the
    // same news: one means the cycle ran and the rollup has nothing in it
    // right now, the other that nothing has been attempted. Without the
    // distinction a rebuild that correctly found zero rows reads as a rebuild
    // that never happened.
    return rollup.empty_since
      ? ({ label: "Empty", tone: "text-muted-foreground", Icon: Circle } as const)
      : ({ label: "Not built", tone: "text-muted-foreground", Icon: ZapOff } as const);
  }
  if (rollup.has_parquet) {
    return { label: "Cached", tone: "font-medium text-primary", Icon: Zap } as const;
  }
  // Built, but not here. Whether that still skips the warehouse depends on
  // whether this deployment has shared storage at all, so the label follows
  // the capability rather than asserting one.
  return blobReads
    ? ({ label: "Built elsewhere", tone: "text-foreground", Icon: Cloud } as const)
    : ({ label: "Not cached here", tone: "text-muted-foreground", Icon: CloudOff } as const);
};

export const CacheState = ({
  rollup,
  blobReads,
  size = "sm"
}: {
  rollup: CacheFacts;
  /**
   * Whether a rollup built on another node is readable from here.
   *
   * Required, and deliberately not defaulted: a falsy default made a missed
   * call site render "Not cached here" on a deployment that does have shared
   * storage — the same rollup contradicting itself between the tab and the
   * view sidebar. Required makes that a type error instead of wrong output.
   */
  blobReads: boolean;
  size?: "sm" | "md";
}) => {
  const icon = size === "md" ? "h-3.5 w-3.5" : "h-3 w-3";
  const { label, tone, Icon } = state(rollup, blobReads);
  return (
    <span className='flex items-center gap-1.5 whitespace-nowrap' title={hint(rollup, blobReads)}>
      <Icon className={`${icon} shrink-0 ${rollup.is_built ? "" : "text-muted-foreground"}`} />
      <span className={tone}>{label}</span>
    </span>
  );
};

/** What each state means for a query, said where someone will read it. */
const hint = (rollup: CacheFacts, blobReads: boolean) => {
  if (!rollup.is_built) {
    return rollup.empty_since
      ? "The last rebuild found no rows, so the rollup was removed rather than left serving the previous build. Queries go to the warehouse."
      : "No rollup has been built; queries go to the warehouse.";
  }
  if (rollup.has_parquet) return "Served from this node's local Parquet.";
  return blobReads
    ? "Built on another node. Queries still skip the warehouse — this node reads the rollup from shared storage."
    : "Built on another node, and this deployment has no shared storage configured, so queries here go to the warehouse.";
};

/** A rollup's dimension/measure names as monospace chips. */
export const FieldChips = ({ items }: { items: string[] }) => {
  if (items.length === 0) return <span className='text-muted-foreground'>—</span>;
  return (
    <div className='flex flex-wrap gap-1'>
      {items.map((item) => (
        <span
          key={item}
          className='rounded bg-muted px-1.5 py-0.5 font-mono text-foreground text-xs'
        >
          {item}
        </span>
      ))}
    </div>
  );
};

/** Measure names with their aggregation type, matching the sidebar's `name (type)`. */
export const MeasureChips = ({ measures }: { measures: PreaggRollupStatus["measures"] }) => {
  if (measures.length === 0) return <span className='text-muted-foreground'>—</span>;
  return (
    <div className='flex flex-wrap gap-1'>
      {measures.map((m) => (
        <span
          key={m.name}
          className='rounded bg-muted px-1.5 py-0.5 font-mono text-foreground text-xs'
        >
          {m.name}
          {m.measure_type && <span className='ml-1 text-muted-foreground'>({m.measure_type})</span>}
        </span>
      ))}
    </div>
  );
};

/**
 * When a rollup was last built: `refresh_key_checked_at` if the worker has
 * checked it, else the manifest's own `build_date`.
 *
 * Both arrive RFC3339 UTC — the server normalizes them (`build_preagg_status`)
 * precisely because the raw manifest value is naive and `new Date()` would read
 * it as viewer-local. An unparseable value still renders an em dash rather than
 * "Invalid Date": an old cache is a normal state, not something to shout about.
 */
export const builtAt = (rollup: PreaggRollupStatus): string | null => {
  // `empty_since` last: a rollup that rebuilt to zero rows has no build time
  // of its own left — the retraction removed the manifest entry carrying it —
  // and the moment it emptied is the honest answer to "when did this last
  // run?", which "Never" is not.
  const raw = rollup.refresh_key_checked_at ?? rollup.build_date ?? rollup.empty_since;
  if (!raw) return null;
  return dayjs(raw).isValid() ? raw : null;
};

export const BuiltAt = ({ rollup }: { rollup: PreaggRollupStatus }) => {
  const built = builtAt(rollup);
  // A declared rollup with no build time has never been built — that's the
  // normal state for a fresh workspace, and worth saying rather than dashing.
  if (!built) {
    return <span className='text-muted-foreground'>{rollup.is_built ? "—" : "Never"}</span>;
  }
  return (
    <>
      <span className='text-foreground'>{formatDate(built)}</span>
      <span className='ml-1 text-muted-foreground'>({timeAgo(built)})</span>
    </>
  );
};

/** Just the icon, for places too narrow for the label. */
export const CacheIcon = ({
  rollup,
  blobReads
}: {
  rollup: CacheFacts;
  /** Required for the same reason as on [`CacheState`]. */
  blobReads: boolean;
}) => {
  const { Icon, tone } = state(rollup, blobReads);
  // The title goes on a wrapper, not the icon: lucide components don't forward
  // `title`, so passing it there is a type error and, worse, silently no help.
  return (
    <span className='inline-flex' title={hint(rollup, blobReads)}>
      <Icon className={`h-3 w-3 shrink-0 ${tone}`} />
    </span>
  );
};
