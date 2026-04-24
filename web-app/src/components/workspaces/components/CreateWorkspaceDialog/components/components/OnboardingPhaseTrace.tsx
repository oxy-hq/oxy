/**
 * Condensed timeline-style trace for onboarding build phases.
 *
 * Renders as a natural-height substep list connected to the phase title above
 * via a left-border timeline connector. No fixed-height card, no empty space.
 *
 * Key correctness rule: when isRunning=false, all steps are force-marked as
 * done regardless of the event stream state (flush() leaves steps open).
 */

import { Check, FileCheck, Loader2, X } from "lucide-react";
import { useMemo } from "react";
import { type AnalyticsStep, buildAnalyticsSteps } from "@/hooks/analyticsSteps";
import { cn } from "@/libs/shadcn/utils";
import type { UiBlock } from "@/services/api/analytics";

// ── Tool label mapping ────────────────────────────────────────────────────────

const TOOL_LABEL: Record<string, string> = {
  execute_sql: "SQL",
  read_file: "read",
  search_files: "search",
  list_files: "list",
  write_file: "write",
  create_file: "write",
  grep_search: "grep",
  web_search: "search",
  lookup_schema: "lookup schema",
  validate_project: "validate project",
  semantic_query: "semantic query"
};

function toolLabel(name: string): string {
  return TOOL_LABEL[name] ?? name.replace(/_/g, " ");
}

// ── Data extraction ───────────────────────────────────────────────────────────

function getProposedFiles(step: AnalyticsStep): string[] {
  const files: string[] = [];
  for (const item of step.items) {
    if (item.kind !== "artifact" || item.toolName !== "propose_change") continue;
    try {
      const input = JSON.parse(item.toolInput);
      if (input?.file_path) files.push(input.file_path as string);
    } catch {
      // ignore unparsable input
    }
  }
  return files;
}

function getToolCounts(step: AnalyticsStep): Array<{ label: string; count: number }> {
  const counts = new Map<string, number>();
  for (const item of step.items) {
    if (item.kind !== "artifact" || item.toolName === "propose_change") continue;
    const label = toolLabel(item.toolName);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([label, count]) => ({ label, count }));
}

function getStepSummary(step: AnalyticsStep): string | null {
  // Prefer the step summary set by step_summary_update events
  if (step.summary?.trim()) return step.summary.trim();
  // Fall back to first text item
  for (const item of step.items) {
    if (item.kind === "text" && item.text.trim()) {
      // Collapse newlines and return the full string — CSS truncation handles display
      return item.text.trim().replace(/\s+/g, " ");
    }
  }
  return null;
}

function shortPath(filePath: string): string {
  const parts = filePath.replace(/\\/g, "/").split("/");
  return parts.slice(-2).join("/");
}

// ── Step row ──────────────────────────────────────────────────────────────────

function StepRow({ step }: { step: AnalyticsStep }) {
  const proposedFiles = useMemo(() => getProposedFiles(step), [step]);
  const toolCounts = useMemo(() => getToolCounts(step), [step]);
  const summary = useMemo(() => getStepSummary(step), [step]);
  const isActive = step.isStreaming;
  const hasError = !!step.error;

  return (
    <div className='flex items-start gap-2 py-1.5'>
      {/* Status icon — fixed width so text always aligns */}
      <div className='mt-0.5 flex w-4 shrink-0 justify-center'>
        {isActive ? (
          <Loader2 className='h-3.5 w-3.5 animate-spin text-primary' />
        ) : hasError ? (
          <X className='h-3.5 w-3.5 text-destructive' />
        ) : (
          <Check className='h-3.5 w-3.5 text-muted-foreground/60' />
        )}
      </div>

      <div className='min-w-0 flex-1'>
        {/* Label + tool pills — wraps cleanly */}
        <div className='flex flex-wrap items-baseline gap-x-1.5 gap-y-1'>
          <span
            className={cn(
              "font-medium text-sm",
              isActive ? "text-foreground" : "text-muted-foreground"
            )}
          >
            {step.label}
          </span>
          {toolCounts.map(({ label, count }) => (
            <span
              key={label}
              className='rounded bg-muted px-1.5 py-px font-mono text-muted-foreground text-xs'
            >
              {label}
              {count > 1 && <span className='opacity-60'>×{count}</span>}
            </span>
          ))}
        </div>

        {/* Summary — single line, graceful ellipsis. Only on active or just-completed step */}
        {summary && (
          <p className='mt-0.5 truncate text-muted-foreground text-xs leading-snug'>{summary}</p>
        )}

        {/* Proposed file badges */}
        {proposedFiles.length > 0 && (
          <div className='mt-1 flex flex-wrap gap-x-3 gap-y-0.5'>
            {proposedFiles.map((f) => (
              <span key={f} className='flex items-center gap-1 text-primary text-xs'>
                <FileCheck className='h-3 w-3 shrink-0' />
                <span className='font-mono'>{shortPath(f)}</span>
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Root ──────────────────────────────────────────────────────────────────────

interface OnboardingPhaseTraceProps {
  events: UiBlock[];
  isRunning: boolean;
}

export default function OnboardingPhaseTrace({ events, isRunning }: OnboardingPhaseTraceProps) {
  const steps = useMemo(() => {
    const raw = buildAnalyticsSteps(events).filter(
      (item): item is AnalyticsStep => item.kind === "step"
    );
    // Fix lingering spinners: when the run has ended, force all steps to completed.
    // buildAnalyticsSteps.flush() leaves open steps with isStreaming=true even
    // after the SSE stream closes — isRunning=false is the authoritative signal.
    if (!isRunning) {
      return raw.map((s) => (s.isStreaming ? { ...s, isStreaming: false } : s));
    }
    return raw;
  }, [events, isRunning]);

  // Nothing yet — don't render anything (no empty placeholder)
  if (steps.length === 0 && !isRunning) return null;

  return (
    // Timeline: left-border acts as a connector under the phase title icon above.
    // ml-8 aligns with OnboardingMessage's text (icon w-5 + gap-3 ≈ 2rem).
    // No fixed height — content determines height.
    <div className='ml-8 border-border border-l pl-4'>
      {steps.length === 0 ? (
        // Initialising — single compact row, no empty space
        <div className='flex items-center gap-2 py-1.5 text-muted-foreground text-xs'>
          <Loader2 className='h-3 w-3 animate-spin text-primary' />
          Starting…
        </div>
      ) : (
        <div className='space-y-0 divide-y divide-border/50'>
          {steps.map((step) => (
            <StepRow key={step.id} step={step} />
          ))}
        </div>
      )}
    </div>
  );
}
