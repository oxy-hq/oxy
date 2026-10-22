import { Badge } from "@/components/ui/shadcn/badge";
import { useAppBuilds } from "@/hooks/api/customApps/useAppBuilds";
import { shortBuildId } from "../../../AppLifecycle/lifecycleCopy";
import { IncompleteSource, SourceLink } from "./SourceLink";

const RTF = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
const TIME_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 31_536_000_000],
  ["month", 2_592_000_000],
  ["day", 86_400_000],
  ["hour", 3_600_000],
  ["minute", 60_000],
  ["second", 1_000]
];

/** "3 hours ago" from an ISO string. Falls back to "" on a bad date. */
function timeAgo(iso: string): string {
  const ms = new Date(iso).getTime();
  if (Number.isNaN(ms)) return "";
  const diff = ms - Date.now();
  const abs = Math.abs(diff);
  for (const [unit, msPer] of TIME_UNITS) {
    if (abs >= msPer || unit === "second") return RTF.format(Math.round(diff / msPer), unit);
  }
  return "";
}

/**
 * The versioned build history of a custom app — every retained build, newest
 * first, with who published it, when, and where its code is.
 *
 * A list and nothing more. It used to carry the live/draft channel cards and a
 * "Make live" button per row, which made it one of four places a pointer move could
 * be made under three different names; the `AppLifecycle` card above it owns those
 * verbs now ("Promote draft", "Roll back to…"), and this is the ledger they choose
 * from. Empty for legacy rows never published via `oxyc publish`.
 */
export const BuildHistory = ({ appId }: { appId: string }) => {
  const { data, isLoading, error } = useAppBuilds(appId);
  const builds = data?.builds;

  if (isLoading) {
    return <p className='text-muted-foreground text-xs'>Loading deployments…</p>;
  }
  if (error) {
    return <p className='text-destructive text-xs'>Couldn't load deployments.</p>;
  }
  if (!builds || builds.length === 0) {
    return (
      <p className='text-muted-foreground text-xs' data-testid='admin-app-builds-empty'>
        No deployments yet — this app hasn't been published via the pipeline (oxyc publish).
      </p>
    );
  }

  return (
    <div className='flex flex-col gap-2' data-testid='admin-app-builds'>
      <h3 className='font-medium text-xs'>Build history</h3>
      <ul className='flex flex-col gap-1'>
        {builds.map((b) => (
          <li
            key={b.id}
            data-testid={`admin-app-build-${shortBuildId(b.build_id)}`}
            className='flex items-center justify-between gap-3 rounded-md border border-border bg-card px-3 py-2'
          >
            <div className='flex min-w-0 flex-col'>
              <div className='flex items-center gap-2'>
                <span className='font-mono text-xs' title={b.build_id}>
                  {shortBuildId(b.build_id)}
                </span>
                {b.is_published && <Badge className='h-5'>Live</Badge>}
                {b.is_draft && !b.is_published && (
                  <Badge variant='secondary' className='h-5'>
                    Draft
                  </Badge>
                )}
              </div>
              <span
                className='truncate text-muted-foreground text-xs'
                title={new Date(b.created_at).toLocaleString()}
              >
                {timeAgo(b.created_at)}
                {b.published_by_email ? ` · ${b.published_by_email}` : ""}
              </span>
              {b.source_repo?.trim() && b.commit_sha?.trim() ? (
                <SourceLink repo={b.source_repo} sha={b.commit_sha} branch={b.source_branch} />
              ) : (
                <IncompleteSource repo={b.source_repo} sha={b.commit_sha} />
              )}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
};
