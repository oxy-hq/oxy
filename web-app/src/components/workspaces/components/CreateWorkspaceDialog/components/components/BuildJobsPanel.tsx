import { Check, ChevronDown, Loader2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { SelectableItem } from "@/hooks/analyticsSteps";
import type { UseAnalyticsRunResult } from "@/hooks/useAnalyticsRun";
import { cn } from "@/libs/shadcn/utils";
import AnalyticsReasoningTrace from "@/pages/thread/analytics/AnalyticsReasoningTrace";
import SuspensionPrompt from "@/pages/thread/analytics/SuspensionPrompt";
import type { UiBlock } from "@/services/api/analytics";
import type { PhaseProgress, SubPhaseKey } from "../types";

export type JobStatus = "queued" | "running" | "done" | "failed";

export interface BuildJob {
  id: SubPhaseKey;
  label: string;
  status: JobStatus;
  /** Events for the reasoning trace (when expanded). */
  events: UiBlock[];
  /** Backing run — used for suspension prompts. Not present for the aggregated semantic job. */
  run?: UseAnalyticsRunResult;
  /** Progress + timing snapshot (monotonic). */
  progress: PhaseProgress;
  /** When a semantic job is running, shows "3 of 4 views". */
  badge?: string;
}

interface BuildJobsPanelProps {
  jobs: BuildJob[];
  isComplete: boolean;
  /** Wall-clock duration since the build started. Freezes on completion. */
  totalElapsedMs: number;
  onSelectArtifact: (item: SelectableItem) => void;
  /** Used to detect non-auto-acceptable suspensions that need a prompt. */
  isAutoAcceptable: (questions: Array<{ prompt: string; suggestions?: string[] }>) => boolean;
}

/**
 * Quiet, orchestration-style panel that replaces the per-phase message stream
 * during the build step. At most one job's reasoning trace is expanded at a time.
 */
export default function BuildJobsPanel({
  jobs,
  isComplete,
  totalElapsedMs,
  onSelectArtifact,
  isAutoAcceptable
}: BuildJobsPanelProps) {
  const doneCount = jobs.filter((j) => j.status === "done").length;
  const failedCount = jobs.filter((j) => j.status === "failed").length;
  const total = jobs.length;
  const overallRatio = total > 0 ? (doneCount + failedCount) / total : 0;

  // Accordion state: `null` = auto-follow the first running job; a concrete id = user pick;
  // `"collapsed"` = user explicitly closed everything.
  type Selection = SubPhaseKey | "collapsed" | null;
  const [selection, setSelection] = useState<Selection>(null);
  const autoFollowId = useMemo(() => jobs.find((j) => j.status === "running")?.id ?? null, [jobs]);

  // When all jobs complete, stop forcing an expansion (nothing live to follow).
  useEffect(() => {
    if (isComplete && selection === null) setSelection("collapsed");
  }, [isComplete, selection]);

  const expandedId =
    selection === null ? autoFollowId : selection === "collapsed" ? null : selection;

  const toggle = (id: SubPhaseKey) => {
    setSelection((prev) => {
      const current = prev === null ? autoFollowId : prev === "collapsed" ? null : prev;
      if (current === id) return "collapsed";
      return id;
    });
  };

  return (
    <div className='overflow-hidden rounded-lg border border-border bg-card/30'>
      {/* Header */}
      <div className='flex items-center gap-3 border-border border-b px-4 py-3'>
        <HeaderIcon isComplete={isComplete} hasFailed={failedCount > 0} />
        <div className='flex-1'>
          <p className='font-medium text-sm'>
            {isComplete ? "Workspace ready" : "Building your workspace"}
          </p>
          <p className='text-muted-foreground text-xs'>
            {isComplete
              ? failedCount > 0
                ? `${doneCount} of ${total} built · ${failedCount} failed`
                : `${total} artifact${total === 1 ? "" : "s"} created`
              : `${doneCount} of ${total} complete${failedCount ? ` · ${failedCount} failed` : ""}`}
          </p>
        </div>
        {totalElapsedMs > 0 && (
          <span className='shrink-0 text-muted-foreground text-xs tabular-nums'>
            {formatDuration(totalElapsedMs)}
          </span>
        )}
      </div>

      {/* Overall progress — a single quiet line */}
      <div className='h-0.5 bg-muted'>
        <div
          className={cn(
            "h-full transition-all duration-700 ease-out",
            isComplete ? "bg-primary" : "bg-primary/60"
          )}
          style={{ width: `${Math.round(overallRatio * 100)}%` }}
        />
      </div>

      {/* Job rows */}
      <div className='flex flex-col'>
        {jobs.map((job, i) => (
          <BuildJobRow
            key={job.id}
            job={job}
            isExpanded={expandedId === job.id}
            isLast={i === jobs.length - 1}
            onToggle={() => toggle(job.id)}
            onSelectArtifact={onSelectArtifact}
            isAutoAcceptable={isAutoAcceptable}
          />
        ))}
      </div>
    </div>
  );
}

function HeaderIcon({ isComplete, hasFailed }: { isComplete: boolean; hasFailed: boolean }) {
  if (isComplete && !hasFailed) {
    return (
      <div className='flex h-7 w-7 items-center justify-center rounded-full bg-primary/10'>
        <Check className='h-3.5 w-3.5 text-primary' />
      </div>
    );
  }
  if (isComplete && hasFailed) {
    return (
      <div className='flex h-7 w-7 items-center justify-center rounded-full bg-destructive/10'>
        <X className='h-3.5 w-3.5 text-destructive' />
      </div>
    );
  }
  return (
    <div className='flex h-7 w-7 items-center justify-center rounded-full bg-primary/10'>
      <Loader2 className='h-3.5 w-3.5 animate-spin text-primary' />
    </div>
  );
}

// ── Row ────────────────────────────────────────────────────────────────────

interface BuildJobRowProps {
  job: BuildJob;
  isExpanded: boolean;
  isLast: boolean;
  onToggle: () => void;
  onSelectArtifact: (item: SelectableItem) => void;
  isAutoAcceptable: (questions: Array<{ prompt: string; suggestions?: string[] }>) => boolean;
}

function BuildJobRow({
  job,
  isExpanded,
  isLast,
  onToggle,
  onSelectArtifact,
  isAutoAcceptable
}: BuildJobRowProps) {
  const canExpand = job.events.length > 0 || job.status === "running";
  const meta = metaFor(job);

  // The step row reads as a disclosure button. Its label uses slightly reduced
  // opacity that lifts to full on hover + when the row is expanded — a quiet
  // affordance that distinguishes it from trace content (always muted).
  const labelTone = cn(
    "text-sm transition-colors",
    job.status === "done" && "text-muted-foreground group-hover:text-foreground",
    job.status === "queued" && "text-muted-foreground/60",
    job.status === "running" && "font-medium text-foreground",
    job.status === "failed" && "text-destructive",
    isExpanded && job.status !== "queued" && "text-foreground"
  );

  return (
    <div>
      <button
        type='button'
        onClick={canExpand ? onToggle : undefined}
        disabled={!canExpand}
        className={cn(
          "group flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors",
          !isLast && "border-border/40 border-b",
          canExpand && "hover:bg-muted/30",
          isExpanded && "bg-muted/20",
          !canExpand && "cursor-default"
        )}
      >
        <JobStatusIcon status={job.status} />
        <div className='min-w-0 flex-1'>
          <div className='flex items-baseline gap-2'>
            <p className={labelTone}>{job.label}</p>
            {job.badge && (
              <span className='shrink-0 text-muted-foreground text-xs tabular-nums'>
                {job.badge}
              </span>
            )}
          </div>
        </div>
        {meta && (
          <span className='shrink-0 text-muted-foreground text-xs tabular-nums'>{meta}</span>
        )}
        {canExpand && (
          <ChevronDown
            className={cn(
              "h-4 w-4 shrink-0 text-muted-foreground/50 transition-transform group-hover:text-muted-foreground",
              isExpanded && "rotate-180 text-muted-foreground"
            )}
          />
        )}
      </button>

      {/* Expanded drill-down: reasoning trace + (rare) suspension prompt.
          Indented under the step icon with a subtle left-rail so the trace
          reads as "content inside this step" rather than a sibling block. */}
      {isExpanded && canExpand && (
        <div className='py-2 pr-4 pl-10'>
          <div className='border-border/40 border-l pl-4'>
            <AnalyticsReasoningTrace
              events={job.events}
              isRunning={job.status === "running"}
              onSelectArtifact={onSelectArtifact}
              defaultCollapsed={false}
              flat
            />
            {job.run?.state.tag === "suspended" && !isAutoAcceptable(job.run.state.questions) && (
              <div className='mt-3'>
                <SuspensionPrompt
                  questions={job.run.state.questions}
                  onAnswer={job.run.answer}
                  isAnswering={job.run.isAnswering}
                />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function JobStatusIcon({ status }: { status: JobStatus }) {
  if (status === "done") {
    return (
      <div className='flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/10'>
        <Check className='h-3 w-3 text-primary' />
      </div>
    );
  }
  if (status === "failed") {
    return (
      <div className='flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-destructive/10'>
        <X className='h-3 w-3 text-destructive' />
      </div>
    );
  }
  if (status === "running") {
    return (
      <div className='flex h-5 w-5 shrink-0 items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin text-primary' />
      </div>
    );
  }
  return (
    <div className='flex h-5 w-5 shrink-0 items-center justify-center'>
      <div className='h-1.5 w-1.5 rounded-full bg-muted-foreground/30' />
    </div>
  );
}

function metaFor(job: BuildJob): string | null {
  // Completed: show the actual duration so the user sees the receipt.
  if (job.status === "done" && job.progress.elapsedSeconds > 0) {
    return formatSeconds(job.progress.elapsedSeconds);
  }
  // Running: the panel header already shows the global elapsed clock, so the
  // per-row counter just adds noise. Surface only the remaining estimate.
  if (job.status === "running") {
    const remaining = Math.max(0, job.progress.estimatedSeconds - job.progress.elapsedSeconds);
    return remaining > 0 ? `~${formatSeconds(remaining)} left` : null;
  }
  return null;
}

function formatSeconds(s: number): string {
  if (s < 60) return `${s}s`;
  const mins = Math.floor(s / 60);
  const secs = s % 60;
  if (secs === 0) return `${mins}m`;
  return `${mins}m ${secs}s`;
}

function formatDuration(ms: number): string {
  return formatSeconds(Math.round(ms / 1000));
}
