import { AlertTriangle, GitBranch } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { useAppBuilds } from "@/hooks/api/customApps/useAppBuilds";
import { cn } from "@/libs/shadcn/utils";
import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import { shortBuildId } from "../../../AppLifecycle/lifecycleCopy";

/** Normalize a git remote URL (`git@host:org/repo.git`, `https://…`, `ssh://…`)
 *  to an https browse URL, or null when unrecognized. */
function repoBrowseUrl(remote: string): string | null {
  const s = remote.trim();
  const ssh = s.match(/^git@([^:]+):(.+?)(?:\.git)?$/);
  if (ssh) return `https://${ssh[1]}/${ssh[2]}`;
  try {
    const u = new URL(s.replace(/^ssh:\/\//, "https://"));
    return `https://${u.host}${u.pathname.replace(/\.git$/, "")}`;
  } catch {
    return null;
  }
}

/** A GitHub-style source link: branch + short commit, linking to the commit
 *  (Vercel-style). Silent when the repo URL can't be normalized. */
const SourceLink = ({
  repo,
  sha,
  branch
}: {
  repo: string;
  sha: string;
  branch?: string | null;
}) => {
  const base = repoBrowseUrl(repo);
  const label = `${branch ? `${branch} · ` : ""}${sha.slice(0, 7)}`;
  const content = (
    <>
      <GitBranch className='size-3 shrink-0' />
      <span className='truncate'>{label}</span>
    </>
  );
  if (!base) {
    return (
      <span className='inline-flex items-center gap-1 text-muted-foreground/70 text-xs'>
        {content}
      </span>
    );
  }
  return (
    <a
      href={`${base}/commit/${sha}`}
      target='_blank'
      rel='noopener noreferrer'
      title={`${branch ? `${branch} @ ` : ""}${sha}`}
      onClick={(e) => e.stopPropagation()}
      className='inline-flex items-center gap-1 text-muted-foreground/80 text-xs hover:text-foreground hover:underline'
    >
      {content}
    </a>
  );
};

/**
 * Shown in `SourceLink`'s place when a build's repo + commit don't both
 * resolve. Reaching the code needs both: a repo without a commit points at a
 * branch, which moves; a commit without a repo has nowhere to resolve.
 *
 * The silence this replaces was the whole problem: a build with no provenance
 * rendered identically to one whose link simply sat further down the row, so
 * an app nobody could trace looked exactly like an app nobody had scrolled
 * to. `oxyc publish` warns about this at publish time; this is where you find
 * the ones that already shipped.
 *
 * Names which half is missing — this row has both fields, so telling an
 * operator "no source recorded" when the repo is right there would send them
 * looking for the wrong thing.
 */
const IncompleteSource = ({ repo, sha }: { repo?: string | null; sha?: string | null }) => {
  const hasRepo = !!repo?.trim();
  const hasSha = !!sha?.trim();
  const label = hasRepo ? "No commit recorded" : hasSha ? "No repo recorded" : "No source recorded";
  const detail = hasRepo
    ? "This build records a git repo but no commit, so the link points at a branch — which moves, and won't identify the code that is running."
    : hasSha
      ? "This build records a commit but no git repo, so there's nowhere to resolve that sha."
      : "This build records no git repo and no commit, so there's no way to get from the running app back to its code.";
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={cn("inline-flex w-fit items-center gap-1 text-xs", ADMIN_TONE.warn.text)}
          data-testid='admin-app-build-no-source'
        >
          <AlertTriangle className='size-3 shrink-0' />
          <span>{label}</span>
        </span>
      </TooltipTrigger>
      <TooltipContent className='max-w-xs'>
        {detail} Re-publish from the app's git checkout to attach provenance.
      </TooltipContent>
    </Tooltip>
  );
};

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
