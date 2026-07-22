// Reasoning trace for the Ask dock — the compact cousin of the web-app's
// AnalyticsReasoningTrace: header meta (LLM calls · time · steps), one row
// per step (status icon · label · summary), and indented item rows —
// tool calls with input previews and durations, thinking text, queries
// with row counts.
//
// Collapse model: the whole trace is expanded while the run streams and
// collapses to the header line when it settles. Within the trace, each
// step shows its item rows while it RUNS and auto-collapses to its header
// once it ends; clicking a step (any time, including after the run) takes
// over that step's toggle.

import { useMemo, useState } from "react";
import { cx } from "./cx";
import { formatMs, type TraceStep } from "./trace";

function StatusIcon({ status }: { status: TraceStep["status"] }) {
  if (status === "running")
    return <span role='status' className='oxy-trace__spin' aria-label='running' />;
  if (status === "failed") return <span className='oxy-trace__icon oxy-trace__icon--fail'>✕</span>;
  if (status === "suspended")
    return <span className='oxy-trace__icon oxy-trace__icon--wait'>…</span>;
  return <span className='oxy-trace__icon oxy-trace__icon--ok'>✓</span>;
}

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      width='12'
      height='12'
      viewBox='0 0 24 24'
      fill='none'
      stroke='currentColor'
      strokeWidth='2'
      strokeLinecap='round'
      strokeLinejoin='round'
      aria-hidden='true'
      style={{ transform: open ? "rotate(90deg)" : undefined, transition: "transform 0.15s" }}
    >
      <path d='m9 18 6-6-6-6' />
    </svg>
  );
}

function TraceItemRows({
  step,
  itemToggles,
  onToggleItem
}: {
  step: TraceStep;
  itemToggles: Record<string, boolean>;
  onToggleItem: (id: string, open: boolean) => void;
}) {
  return (
    <div className='oxy-trace__items'>
      {step.items.map((item) => {
        if (item.kind === "thinking") {
          return (item.text ?? "").trim() ? (
            <p key={item.id} className='oxy-trace__think'>
              {item.text}
            </p>
          ) : null;
        }
        const hasDetail = Boolean(item.input || item.output);
        // Same collapse model as steps: detail is visible while the action
        // runs, folds when it completes, and a click wins from then on.
        const itemOpen = hasDetail && (itemToggles[item.id] ?? item.running === true);
        const row = (
          <>
            {item.running && (
              <span
                role='status'
                className='oxy-trace__spin oxy-trace__spin--sm'
                aria-label='running'
              />
            )}
            <span className='oxy-trace__row-label'>{item.label}</span>
            {item.preview && <span className='oxy-trace__row-preview'>{item.preview}</span>}
            {item.detail && <span className='oxy-trace__row-detail'>{item.detail}</span>}
          </>
        );
        return (
          <div key={item.id} className='oxy-trace__item'>
            {hasDetail ? (
              <button
                type='button'
                className={cx(
                  "oxy-trace__row",
                  "oxy-trace__row--btn",
                  item.error && "oxy-trace__row--error"
                )}
                onClick={() => onToggleItem(item.id, !itemOpen)}
                aria-expanded={itemOpen}
                data-testid={`reasoning-pill-${item.label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`}
              >
                {row}
                <span className='oxy-trace__step-chevron'>
                  <ChevronIcon open={itemOpen} />
                </span>
              </button>
            ) : (
              <div
                className={cx("oxy-trace__row", item.error && "oxy-trace__row--error")}
                data-testid={`reasoning-pill-${item.label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`}
              >
                {row}
              </div>
            )}
            {itemOpen && (
              <div className='oxy-trace__payloads'>
                {item.input && <pre className='oxy-trace__payload'>{item.input}</pre>}
                {item.output && (
                  <pre className='oxy-trace__payload oxy-trace__payload--out'>{item.output}</pre>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

export interface ReasoningTraceProps {
  steps: TraceStep[];
  /** True while the run is still streaming — keeps the trace expanded. */
  streaming?: boolean;
  /** Header meta from `aggregateLlmStats` (LLM call count + total time). */
  llm?: { calls: number; totalMs: number };
  className?: string;
}

/** Renders nothing when there are no steps (runs whose stream carries no
 *  step events degrade to the plain answer, same as before). */
export function ReasoningTrace({ steps, streaming = false, llm, className }: ReasoningTraceProps) {
  const [userToggled, setUserToggled] = useState<boolean | null>(null);
  // Per-step / per-action overrides: absent → automatic (open while
  // running, collapsed once ended); present → the user's choice wins.
  const [stepToggles, setStepToggles] = useState<Record<string, boolean>>({});
  const [itemToggles, setItemToggles] = useState<Record<string, boolean>>({});

  // A new run (first step identity changes) returns every toggle to
  // automatic — render-time reset, the React-documented pattern for
  // adjusting state when a prop changes.
  const runKey = steps[0]?.id ?? "";
  const [prevRunKey, setPrevRunKey] = useState(runKey);
  if (prevRunKey !== runKey) {
    setPrevRunKey(runKey);
    setUserToggled(null);
    setStepToggles({});
    setItemToggles({});
  }

  // Expanded while streaming; collapses when the run settles — unless the
  // user has taken over the toggle.
  const open = userToggled ?? streaming;

  const meta = useMemo(() => {
    const parts: string[] = [];
    if (llm && llm.calls > 0) {
      parts.push(`${llm.calls} LLM ${llm.calls === 1 ? "call" : "calls"}`);
      if (llm.totalMs > 0) parts.push(formatMs(llm.totalMs));
    }
    parts.push(`${steps.length} step${steps.length === 1 ? "" : "s"}`);
    if (steps.some((s) => s.status === "failed")) parts.push("failed");
    return parts.join(" · ");
  }, [llm, steps]);

  if (steps.length === 0) return null;

  return (
    <div className={cx("oxy-trace", className)} data-run={runKey}>
      <button
        type='button'
        className='oxy-trace__head'
        onClick={() => setUserToggled(!open)}
        aria-expanded={open}
      >
        <ChevronIcon open={open} />
        <span className='oxy-trace__title'>Reasoning trace</span>
        <span className='oxy-trace__meta'>{meta}</span>
        {streaming && <span role='status' className='oxy-trace__spin' aria-label='running' />}
      </button>
      {open && (
        <div className='oxy-trace__body'>
          {steps.map((step) => {
            const stepOpen = stepToggles[step.id] ?? step.status === "running";
            if (step.items.length === 0) {
              return (
                <div key={step.id} className='oxy-trace__step'>
                  <div className='oxy-trace__step-row'>
                    <StatusIcon status={step.status} />
                    <span className='oxy-trace__label'>{step.label}</span>
                    {step.summary && <span className='oxy-trace__summary'>{step.summary}</span>}
                  </div>
                </div>
              );
            }
            return (
              <div key={step.id} className='oxy-trace__step'>
                <button
                  type='button'
                  className='oxy-trace__step-row oxy-trace__step-row--btn'
                  onClick={() => setStepToggles((t) => ({ ...t, [step.id]: !stepOpen }))}
                  aria-expanded={stepOpen}
                >
                  <StatusIcon status={step.status} />
                  <span className='oxy-trace__label'>{step.label}</span>
                  {step.summary && <span className='oxy-trace__summary'>{step.summary}</span>}
                  {!stepOpen && (
                    <span className='oxy-trace__count'>
                      {step.items.length} action{step.items.length === 1 ? "" : "s"}
                    </span>
                  )}
                  <span className='oxy-trace__step-chevron'>
                    <ChevronIcon open={stepOpen} />
                  </span>
                </button>
                {stepOpen && (
                  <TraceItemRows
                    step={step}
                    itemToggles={itemToggles}
                    onToggleItem={(id, next) => setItemToggles((t) => ({ ...t, [id]: next }))}
                  />
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
