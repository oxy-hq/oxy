import { CircleCheck, GitCommit, Hammer, Loader2, TriangleAlert } from "lucide-react";
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

/**
 * Manual Compile button in the IDE header next to Pull / Commit & Push.
 *
 * The button is a status surface that also affords action. It's never
 * "just a button" — even at rest it tells the reader (a) what the
 * shipped revision is, (b) whether new commits are sitting on the
 * default branch unshipped, and (c) when the last compile happened.
 *
 * Five rendering states, each driven by metadata returned by
 * `/compile/status`:
 *   - **never**:  no revision yet. Brand-blue accent — call to action.
 *   - **fresh**:  HEAD === latest ready revision. Muted; nothing to do.
 *   - **stale**:  HEAD differs from latest ready revision. Amber accent
 *                 — the user has commits that haven't shipped.
 *   - **compiling**: a revision is running. Pulsing blue.
 *   - **failed**: last compile failed. Destructive border.
 *
 * Blank / demo / no-remote workspaces have `head_sha = null`; they
 * collapse into a single "Compile working tree" affordance with no
 * freshness concept.
 *
 * Disabled when the active branch isn't the workspace's default —
 * non-default branches read from FS, so compiling them would just
 * pollute the revision history.
 */
export function CompileButton() {
  const { project, branchName } = useCurrentProjectBranch();
  const workspaceId = project.id;

  const { data: status, isLoading } = useCompileStatus(workspaceId, branchName);
  const enqueue = useEnqueueCompile(workspaceId, branchName);

  const view = deriveView(status, branchName);
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
        view.kind === "fresh" && "hover:bg-accent/60"
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
            view.kind === "fresh" && "text-muted-foreground",
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
            <CompileTooltipBody view={view} status={status} branchName={branchName} />
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </CanWorkspaceEditor>
  );
}

// ── derived view ────────────────────────────────────────────────────

type ViewKind = "never" | "fresh" | "stale" | "compiling" | "failed" | "no-git";

interface View {
  kind: ViewKind;
  verb: string;
  /** Short SHA chip text, e.g. `a3f4d12` or `a3f4d12 ↑`. Empty = hide chip. */
  sha: string | null;
}

function deriveView(status: CompileStatus | undefined, branchName: string): View {
  if (!status) {
    return { kind: "never", verb: "Compile", sha: null };
  }
  // Disabled-on-non-default branch is a status the button still
  // shows, but the verb explains why the click won't help.
  if (!status.can_compile && status.default_branch && branchName !== status.default_branch) {
    return {
      kind: "fresh",
      verb: "Compile (main only)",
      sha: status.head_sha ? short(status.head_sha) : null
    };
  }

  const latest = status.latest;
  if (latest?.status === "compiling") {
    return {
      kind: "compiling",
      verb: "Compiling…",
      sha: latest.git_sha ? short(latest.git_sha) : null
    };
  }
  if (latest?.status === "failed") {
    return {
      kind: "failed",
      verb: "Retry compile",
      sha: latest.git_sha ? short(latest.git_sha) : null
    };
  }

  // Blank / demo / no-remote — no SHA to track freshness against.
  if (!status.head_sha) {
    return { kind: "no-git", verb: "Compile", sha: null };
  }

  const lastReadySha = latest?.status === "ready" ? latest.git_sha : null;
  if (!lastReadySha) {
    return { kind: "never", verb: "Compile", sha: short(status.head_sha) };
  }
  if (lastReadySha === status.head_sha) {
    return {
      kind: "fresh",
      verb: "Up to date",
      sha: short(status.head_sha)
    };
  }
  return {
    kind: "stale",
    verb: "Compile",
    sha: `${short(status.head_sha)} ↑`
  };
}

// ── visual atoms ────────────────────────────────────────────────────

function StateIcon({ kind, loading }: { kind: ViewKind; loading: boolean }) {
  if (loading || kind === "compiling") {
    return <Loader2 className='size-3 animate-spin' />;
  }
  if (kind === "fresh") return <CircleCheck className='size-3 text-emerald-600/80' />;
  if (kind === "failed") return <TriangleAlert className='size-3 text-destructive' />;
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
      return "bg-emerald-500/60";
    default:
      return "bg-muted-foreground/40";
  }
}

// ── tooltip body ────────────────────────────────────────────────────

function CompileTooltipBody({
  view,
  status,
  branchName
}: {
  view: View;
  status: CompileStatus | undefined;
  branchName: string;
}) {
  if (
    status &&
    !status.can_compile &&
    status.default_branch &&
    branchName !== status.default_branch
  ) {
    return (
      <div className='space-y-1 text-xs'>
        <div>Compile ships from the default branch ({status.default_branch}).</div>
        <div className='text-muted-foreground'>
          You're on <span className='font-mono'>{branchName}</span>. Switch branches to compile.
        </div>
      </div>
    );
  }

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
          <div>Up to date with main</div>
          {describeLatestRevision(status?.latest)}
        </div>
      );
    case "stale":
      return (
        <div className='space-y-1 text-xs'>
          <div>New commits on main haven't shipped yet.</div>
          <div className='text-muted-foreground'>
            HEAD <span className='font-mono'>{short(status?.head_sha ?? "")}</span>
            {status?.latest?.git_sha ? (
              <>
                {" · last shipped "}
                <span className='font-mono'>{short(status.latest.git_sha)}</span>
              </>
            ) : null}
          </div>
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

function describeLatestRevision(latest: RevisionSummary | null | undefined) {
  if (!latest) return null;
  return (
    <div className='text-muted-foreground'>
      Last compiled {latest.finished_at ? relative(latest.finished_at) : "—"}
    </div>
  );
}

function short(sha: string): string {
  return sha.length > 7 ? sha.slice(0, 7) : sha;
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
