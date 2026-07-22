import { CircleCheck, CircleDashed, GitCommit, Hammer, Loader2, TriangleAlert } from "lucide-react";
import { CanWorkspaceEditor } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";
import { useCompileStatus, useEnqueueCompile } from "@/hooks/api/compile";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import type { CompileStatus, RevisionSummary } from "@/services/api/compile";
import { deriveView, short, type View, type ViewKind } from "./deriveCompileView";

/**
 * Manual Compile button in the IDE header next to Pull / Commit & Push.
 *
 * The button is a status surface that also affords action. It's never
 * "just a button" — even at rest it answers "is what I merged live?", so its
 * resting label is load-bearing and must never overstate what is known.
 *
 * Rendering states, each driven by metadata returned by `/compile/status`:
 *   - **never**:  nothing promoted yet. Brand-blue accent — call to action.
 *   - **fresh**:  serving revision === origin tip. Muted; nothing to do.
 *   - **ahead**:  serving revision has origin's tip as an ancestor plus unpushed
 *                 local commits (what a restore leaves behind). Nothing on
 *                 origin is waiting to ship, so this is a resting state —
 *                 emphatically not "behind".
 *   - **stale**:  origin has moved past the serving revision. Amber accent.
 *   - **unverified**: serving a revision, but the origin tip is unknown or the
 *                 last fetch is too old to claim freshness from. Muted, and
 *                 pointedly *not* a check mark.
 *   - **compiling**: a revision is running. Pulsing blue.
 *   - **failed**: last compile failed. Destructive border.
 *
 * `head_sha` (the working copy) is shown for context but never used to decide
 * freshness — see `deriveView` for why that comparison is circular.
 *
 * Blank / demo / no-remote workspaces have `head_sha = null`; they
 * collapse into a single "Compile working tree" affordance with no
 * freshness concept.
 *
 * Hidden entirely when:
 *   - the deployment is single-instance / `all` (`status.boundary_active` is
 *     false) — it serves from the working copy, so compile is a no-op there; or
 *   - the active branch isn't the workspace's default — compile ships only from
 *     the default branch, so the button would do nothing useful on a draft
 *     branch (which reads from FS). See oxygen-internal#2528.
 */
export function CompileButton() {
  const { project, branchName } = useCurrentProjectBranch();
  const workspaceId = project.id;

  const { data: status, isLoading } = useCompileStatus(workspaceId, branchName);
  const enqueue = useEnqueueCompile(workspaceId, branchName);

  // Single-instance / `all` deployments (e.g. `oxy start`, `oxy serve --local`)
  // serve reads from the working copy directly, so a manual compile changes
  // nothing they serve — a no-op. Hide the button entirely; it only earns its
  // place on a multi-instance (split-fleet) deployment where compile promotes
  // the revision the serve fleet reads. See internal-docs/multi-instance-fleet.md.
  if (status && !status.boundary_active) {
    return null;
  }

  // Compile ships only from the default branch — on any other branch the button
  // can't do anything useful, so don't render it at all. (`can_compile` is false
  // here specifically because of the branch.)
  if (
    status &&
    !status.can_compile &&
    status.default_branch &&
    branchName !== status.default_branch
  ) {
    return null;
  }

  const view = deriveView(status);
  const isDisabled = !status?.can_compile || view.kind === "compiling" || enqueue.isPending;

  const handleClick = () => {
    if (!isDisabled) enqueue.mutate();
  };

  const button = (
    <Button
      size='sm'
      variant='outline'
      onClick={handleClick}
      disabled={isDisabled}
      data-testid='ide-compile-button'
      className={cn(
        "group relative h-7 gap-1 overflow-hidden px-2 text-xs transition-colors duration-200",
        accentClassName(view.kind),
        view.kind === "stale" && "hover:bg-accent",
        (view.kind === "fresh" || view.kind === "ahead") && "hover:bg-accent/60"
      )}
    >
      {/* status-driven left accent stripe — the build-tool indicator DNA */}
      <span
        aria-hidden
        className={cn(
          "absolute top-1/2 left-0 h-3/4 w-[2px] -translate-y-1/2 rounded-r transition-colors duration-200",
          stripeClassName(view.kind),
          view.kind === "compiling" && "animate-pulse"
        )}
      />
      <StateIcon kind={view.kind} loading={enqueue.isPending || isLoading} />
      <span className='font-medium'>{view.verb}</span>
      {view.sha ? (
        <span
          className={cn(
            "ml-0.5 inline-flex items-center gap-1 rounded-sm border border-border/60 bg-muted/40 px-1 py-px font-mono text-[10px] text-muted-foreground transition-colors duration-200",
            view.kind === "stale" && "border-amber-500/40 text-amber-700 dark:text-amber-300",
            (view.kind === "fresh" || view.kind === "ahead") && "text-muted-foreground",
            view.kind === "compiling" && "animate-pulse border-primary/40 text-primary"
          )}
        >
          <GitCommit className='size-2.5 opacity-70' />
          {view.sha}
        </span>
      ) : null}
    </Button>
  );

  return (
    <CanWorkspaceEditor>
      <TooltipProvider>
        <Tooltip delayDuration={200}>
          <TooltipTrigger asChild>
            <span>{button}</span>
          </TooltipTrigger>
          <TooltipContent side='bottom' className='max-w-xs'>
            <CompileTooltipBody view={view} status={status} />
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </CanWorkspaceEditor>
  );
}

// ── visual atoms ────────────────────────────────────────────────────

function StateIcon({ kind, loading }: { kind: ViewKind; loading: boolean }) {
  if (loading || kind === "compiling") {
    return <Loader2 className='size-3 animate-spin' />;
  }
  if (kind === "fresh" || kind === "ahead")
    return <CircleCheck className='size-3 text-emerald-600/80' />;
  if (kind === "failed") return <TriangleAlert className='size-3 text-destructive' />;
  // `unverified` deliberately does NOT get the check mark — the tick is the
  // single glyph people read as "your change is live", and this state cannot
  // vouch for that.
  if (kind === "unverified") return <CircleDashed className='size-3 text-muted-foreground' />;
  return <Hammer className='size-3' />;
}

function accentClassName(kind: ViewKind): string {
  switch (kind) {
    case "never":
      return "border-primary/40 text-primary hover:border-primary";
    case "stale":
      return "border-amber-500/50 text-amber-900 dark:text-amber-100 hover:border-amber-500";
    case "compiling":
      return "border-primary/40 text-primary";
    case "failed":
      return "border-destructive/50 text-destructive hover:border-destructive";
    default:
      return "";
  }
}

function stripeClassName(kind: ViewKind): string {
  switch (kind) {
    case "never":
      return "bg-primary";
    case "stale":
      return "bg-amber-500";
    case "compiling":
      return "bg-primary";
    case "failed":
      return "bg-destructive";
    case "fresh":
    case "ahead":
      return "bg-emerald-500/60";
    case "unverified":
      return "bg-muted-foreground/50";
    default:
      return "bg-muted-foreground/40";
  }
}

// ── tooltip body ────────────────────────────────────────────────────

function CompileTooltipBody({ view, status }: { view: View; status: CompileStatus | undefined }) {
  switch (view.kind) {
    case "compiling":
      return (
        <div className='space-y-1 text-xs'>
          <div>Compiling now…</div>
          {status?.latest?.git_sha ? (
            <div className='font-mono text-muted-foreground'>{short(status.latest.git_sha)}</div>
          ) : null}
        </div>
      );
    case "failed":
      return (
        <div className='space-y-1 text-xs'>
          <div>Last compile failed</div>
          {status?.latest?.finished_at ? (
            <div className='text-muted-foreground'>{relative(status.latest.finished_at)}</div>
          ) : null}
          <div className='text-muted-foreground'>Click to retry.</div>
        </div>
      );
    case "fresh":
      return (
        <div className='space-y-1 text-xs'>
          <div>Serving the latest commit on {status?.default_branch ?? "main"}.</div>
          <ShaBreakdown status={status} />
          {describeLatestRevision(status?.latest)}
        </div>
      );
    case "ahead":
      return (
        <div className='space-y-1 text-xs'>
          <div>
            Nothing on origin is waiting to ship. Serving {status?.compiled_ahead ?? 0} commit
            {(status?.compiled_ahead ?? 0) === 1 ? "" : "s"} on top of{" "}
            {status?.default_branch ?? "main"} that{" "}
            {(status?.compiled_ahead ?? 0) === 1 ? "is" : "are"} not pushed.
          </div>
          {/* Deliberately not "everything on origin is live": these commits
              contain origin's as ancestors, but a restore's tree can revert
              their content. Ancestry is what we measured, so ancestry is what
              we state. */}
          <ShaBreakdown status={status} />
        </div>
      );
    case "unverified":
      return (
        <div className='space-y-1 text-xs'>
          <div>Serving a compiled revision — not verified against origin.</div>
          <div className='text-muted-foreground'>
            {status?.remote_sha
              ? `Last fetched ${status.remote_fetched_at ? relative(status.remote_fetched_at) : "a while ago"}; origin may have moved since.`
              : "This clone has not fetched from origin, so the remote tip is unknown."}
          </div>
          <ShaBreakdown status={status} />
        </div>
      );
    case "stale":
      return (
        <div className='space-y-1 text-xs'>
          <div>Origin has commits that aren't being served yet.</div>
          <ShaBreakdown status={status} />
        </div>
      );
    case "never":
      return (
        <div className='space-y-1 text-xs'>
          <div>This workspace hasn't been compiled yet.</div>
          {status?.head_sha ? (
            <div className='text-muted-foreground'>
              Will ship <span className='font-mono'>{short(status.head_sha)}</span>
            </div>
          ) : null}
        </div>
      );
    default:
      return (
        <div className='space-y-1 text-xs'>
          <div>Compile a snapshot of the working tree.</div>
          {describeLatestRevision(status?.latest)}
        </div>
      );
  }
}

/**
 * The three SHAs, always labelled and never conflated. Collapsing them into one
 * chip is what made "Up to date 0c9ad8f" unfalsifiable — the reader could not
 * tell which of the three that SHA was, and it happened to be the only one that
 * did not answer their question.
 */
function ShaBreakdown({ status }: { status: CompileStatus | undefined }) {
  if (!status) return null;
  const rows: Array<[string, string | null]> = [
    ["serving", status.compiled_sha],
    ["origin", status.remote_sha],
    ["files", status.head_sha]
  ];
  return (
    <div className='space-y-0.5 text-muted-foreground'>
      {rows.map(([label, sha]) => (
        <div key={label} className='flex gap-2'>
          <span className='w-12 shrink-0'>{label}</span>
          <span className='font-mono'>{sha ? short(sha) : "—"}</span>
        </div>
      ))}
    </div>
  );
}

function describeLatestRevision(latest: RevisionSummary | null | undefined) {
  if (!latest) return null;
  return (
    <div className='text-muted-foreground'>
      Last compiled {latest.finished_at ? relative(latest.finished_at) : "—"}
    </div>
  );
}

function relative(iso: string): string {
  const ts = new Date(iso).getTime();
  if (Number.isNaN(ts)) return "earlier";
  const ageSecs = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (ageSecs < 60) return "just now";
  if (ageSecs < 60 * 60) return `${Math.floor(ageSecs / 60)}m ago`;
  if (ageSecs < 60 * 60 * 24) return `${Math.floor(ageSecs / 3600)}h ago`;
  return `${Math.floor(ageSecs / 86400)}d ago`;
}
