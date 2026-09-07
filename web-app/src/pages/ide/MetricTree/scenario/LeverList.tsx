import { X } from "lucide-react";
import { useEffect, useState } from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Slider } from "@/components/ui/shadcn/slider";
import { cn } from "@/libs/shadcn/utils";
import type { FittedDriver, MetricTree } from "@/types/metricTree";
import { formatNumber, shortMeasureName } from "@/utils/measureFormat";
import { measureTitle } from "../measureTitle";
import { DriverSizing } from "./DriverSizing";
import type { LeverConflict } from "./leverConflicts";
import { percentFromRaw, rawFromPercent, SLIDER_RANGE } from "./leverPercent";
import { MeasureChange } from "./MeasureChange";
import type { ScenarioNodeData } from "./nodeValue";
import { leverOutsideAnchor, viewOf } from "./pickTimeDimension";
import type { LeverError } from "./resolveLever";
import type { ScenarioState, ScenarioUpdate } from "./scenarioUrl";

interface LeverListProps {
  tree: MetricTree;
  state: ScenarioState;
  onChange: (next: ScenarioUpdate) => void;
  conflicts: LeverConflict[];
  leverErrors: Map<string, LeverError>;
  /** A failed baseline fetch or propagation. Rendered rather than swallowed:
   *  a silent failure is indistinguishable from "this lever moves nothing",
   *  which is what made a broken scenario look like an empty one. */
  requestError?: Error | null;
  /** The server's reason the baseline could not value everything. Reported at
   *  the top rather than only as per-node text, and explicitly NOT as "moves
   *  nothing" — propagation still works, it just can't be anchored to
   *  current values. */
  baselineNote?: string | null;
  /** Whether the baseline valued anything. Decides which sentence the note
   *  above gets: a note can accompany a perfectly usable set of baselines
   *  (one view read, another skipped), so "no baseline to anchor to" is only
   *  ever true when this is false. */
  anyValued?: boolean;
  /** Baseline + per-node render state, keyed by node id — the same map
   *  `useScenario` hands to the canvas, reused here for the baseline figure
   *  shown next to each lever's input. */
  nodeData: Map<string, ScenarioNodeData>;
  /** Coefficients the baseline measured from history, and the ones it
   *  declined to. Both are worth showing: a fitted number is the basis of
   *  every downstream figure, and a refusal is the reason a branch of the
   *  canvas shows nothing at all. */
  fitted?: FittedDriver[];
}

const ERROR_COPY: Record<LeverError, string> = {
  zero_baseline: "baseline is 0, so a % has nothing to scale — use +3 or -3",
  no_baseline: "no baseline value, so only a signed delta (+3) works here",
  not_a_number: "not a number",
  no_change: "" // filtered out upstream; never rendered
};

/**
 * A server note rendered with its markdown backticks as code spans.
 *
 * The notes name identifiers (`` `stores.business_date` ``) and JSX has no
 * markdown, so the backticks were reaching the screen as literal characters.
 */
function Backticked({ text }: { text: string }) {
  return (
    <>
      {text.split("`").map((part, i) =>
        i % 2 === 1 ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: split position IS the identity here
          <span key={i} className='font-mono'>
            {part}
          </span>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: split position IS the identity here
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}

/**
 * Pinned levers: one row per lever with a value input, plus the conflict
 * banner and the two modelling-gap notices that have no other home in the
 * design (a lever that drives nothing, a lever whose measure vanished).
 */
export function LeverList({
  tree,
  state,
  onChange,
  conflicts,
  leverErrors,
  requestError,
  baselineNote,
  anyValued = false,
  nodeData,
  fitted
}: LeverListProps) {
  const vanishedIds = useDropVanishedLevers(tree, state, onChange);

  // Updaters, not spreads of this render's `state`. A keystroke committing in
  // the same tick as the time-dimension adopt effect would otherwise revert
  // `timeDimension` to null — self-healing, since the effect re-fires, but at
  // the cost of an extra baseline round trip. Every write to scenario state
  // takes this form; a snapshot spread anywhere re-opens the class.
  function updateLeverRaw(nodeId: string, raw: string) {
    onChange((prev) => ({
      ...prev,
      levers: prev.levers.map((l) => (l.nodeId === nodeId ? { ...l, raw } : l))
    }));
  }

  function unpin(nodeId: string) {
    onChange((prev) => ({
      ...prev,
      levers: prev.levers.filter((l) => l.nodeId !== nodeId)
    }));
  }

  // `!= null` throughout, not `!== undefined`: `coefficient` is an
  // `Option<f64>` on the wire, and whether it arrives absent or as an explicit
  // `null` is a serde attribute on a git-pinned struct. Under `!== undefined` a
  // pin that stopped skipping nulls would put every REFUSAL in `fits` and every
  // FIT in `refusedFrom` — each edge rendering as the opposite of what it is,
  // with no error anywhere. `!= null` is correct under both.
  const quantified = (f: FittedDriver) => f.coefficient != null;

  // A lever with edges that all failed to fit moves nothing just as surely as
  // one with no edges at all. Testing only for edges left the commonest case —
  // a qualitative driver — with no explanation anywhere on the surface.
  //
  // But `fitted` only ever carries edges the YAML left UNdeclared — a declared
  // `coefficient:` is never measured, so it never appears here at all. Asking
  // `fitted` whether a lever has a working edge therefore cannot see the
  // declared drivers and component expressions that propagate perfectly well,
  // and a lever with one refused fit alongside three declared edges was being
  // told it "moves nothing". The tree is what knows about those, so ask it.
  const refusedFrom = new Set((fitted ?? []).filter((f) => !quantified(f)).map((f) => f.from));
  const quantifiedFrom = new Set((fitted ?? []).filter(quantified).map((f) => f.from));
  const propagatesFrom = (nodeId: string) =>
    quantifiedFrom.has(nodeId) ||
    tree.edges.some((e) => e.from === nodeId && (e.kind !== "driver" || e.coefficient != null));
  const noDriverLevers = state.levers.filter(
    (l) =>
      !tree.edges.some((e) => e.from === l.nodeId) ||
      (refusedFrom.has(l.nodeId) && !propagatesFrom(l.nodeId))
  );
  const refusals = (fitted ?? []).filter((f) => f.refusal);
  const fits = (fitted ?? []).filter(quantified);

  if (state.levers.length === 0 && vanishedIds.length === 0) {
    return (
      <p className='p-4 text-muted-foreground text-xs'>
        Click a measure in the graph to pin it as a lever.
      </p>
    );
  }

  return (
    <div className='flex flex-col gap-3 p-4'>
      {conflicts.length > 0 && (
        <div
          className='border border-destructive/40 bg-destructive/5 p-2'
          data-testid='scenario-conflict'
        >
          <p className='font-mono text-[10px] text-destructive'>Conflicting levers</p>
          {conflicts.map((c) => (
            <div key={`${c.upstream}->${c.downstream}`} className='mt-1 flex flex-col gap-1'>
              <p className='font-mono text-[9.5px] text-muted-foreground'>
                {shortMeasureName(c.downstream)} is downstream of {shortMeasureName(c.upstream)}, so
                the model can't tell whether it holds at your value or moves to the implied one.
              </p>
              <div className='flex gap-2'>
                <button
                  type='button'
                  className='font-mono text-[9.5px] text-destructive underline'
                  onClick={() => unpin(c.upstream)}
                >
                  unpin {shortMeasureName(c.upstream)}
                </button>
                <button
                  type='button'
                  className='font-mono text-[9.5px] text-destructive underline'
                  onClick={() => unpin(c.downstream)}
                >
                  unpin {shortMeasureName(c.downstream)}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {requestError && (
        <p
          className='text-[11px] text-destructive leading-relaxed'
          data-testid='scenario-request-error'
        >
          Couldn't run the simulation: {requestError.message}
        </p>
      )}

      {baselineNote && !requestError && (
        <p
          className={cn(
            "text-[11px] leading-relaxed",
            // A rejected query is a failure, and it rendered muted while the
            // HTTP failure two lines up rendered destructive — the same class
            // of problem in two colours. Only the "no rows / wrong dimension"
            // notes are informational.
            baselineNote.startsWith("the warehouse rejected")
              ? "text-destructive"
              : "text-muted-foreground"
          )}
          data-testid='scenario-baseline-failed'
        >
          {/* Two sentences, because a note does NOT imply an empty baseline.
              The server builds it from the engine outcome AND the views the
              read skipped, so the commonest shape is a note sitting next to
              measures that valued perfectly well — one view read, its
              neighbour lacking the window's time dimension. Claiming "no
              baseline" there contradicted the % lever resolving beside it.

              Only in the genuinely empty case is the stronger sentence true,
              and it is not "shown as relative changes": without a baseline an
              absolute target has nothing to subtract from and a % has nothing
              to scale, so those levers do not resolve at all. Which levers
              those are is already said per-lever by `ERROR_COPY`. */}
          {anyValued
            ? "Part of the baseline is missing — "
            : "No baseline to anchor to, so only signed-delta levers (+3) can be sized — "}
          <Backticked text={baselineNote} />.
        </p>
      )}

      {/* One line unless asked. Every fitted edge and every refusal has a
          sentence, so on a real tree this block ran longer than the panel and
          pushed the levers themselves off the bottom. */}
      <DriverSizing fits={fits} refusals={refusals} />

      {noDriverLevers.map((l) => {
        const node = tree.nodes.find((n) => n.id === l.nodeId);
        const label = node ? measureTitle(node) : l.nodeId;
        const hasEdges = tree.edges.some((e) => e.from === l.nodeId);
        return (
          <p key={l.nodeId} className='text-[11px] text-muted-foreground leading-relaxed'>
            <span className='font-medium text-foreground'>{label}</span>{" "}
            {hasEdges ? (
              <>
                moves nothing this scenario can size — open the driver edges above for the reason,
                or declare a <span className='font-mono'>coefficient:</span> on its{" "}
                <span className='font-mono'>drivers:</span> entry to state the magnitude directly.
              </>
            ) : (
              <>
                drives no modelled metric — add a <span className='font-mono'>drivers:</span> entry
                to its <span className='font-mono'>.view.yml</span> to model what it moves.
              </>
            )}
          </p>
        );
      })}

      {vanishedIds.map((id) => (
        <p key={id} className='text-[11px] text-muted-foreground leading-relaxed'>
          <span className='font-mono'>{id}</span> is no longer in the metric tree and was unpinned.
        </p>
      ))}

      {/* The tree builder's own refusals. An edge whose `coefficients:`
          vector is the wrong width for its `form:` stays qualitative WITH a
          reason attached — and until this rendered, that reason reached
          nobody, leaving the author with a lever that moves nothing and
          nothing on screen saying why. Only the ones naming a pinned lever,
          so an unrelated modelling error elsewhere in the layer does not
          crowd the panel. */}
      {(tree.warnings ?? [])
        .filter((w) => state.levers.some((l) => w.includes(l.nodeId)))
        .map((w) => (
          <p
            key={w}
            className='text-[11px] text-muted-foreground leading-relaxed'
            data-testid='scenario-tree-warning'
          >
            {w}
          </p>
        ))}

      <ul className='flex flex-col gap-2' data-testid='scenario-lever-list'>
        {state.levers.map((lever) => {
          const data = nodeData.get(lever.nodeId);
          const node = tree.nodes.find((n) => n.id === lever.nodeId);
          const error = leverErrors.get(lever.nodeId);
          // `no_baseline` has two very different causes and one of them names
          // a fix the generic copy cannot. `resolveLever` only sees that the
          // value is missing; the WINDOW is why it is missing whenever this
          // lever sits outside the anchored view — and nothing else on the
          // panel says so, because the picker's `foreign` check passes on a
          // dimension belonging to some other lever's view.
          const errorText =
            error === "no_baseline" && leverOutsideAnchor(lever.nodeId, state.timeDimension)
              ? `the window is on \`${viewOf(state.timeDimension ?? "")}\`, not ` +
                `\`${viewOf(lever.nodeId)}\` — move the time dimension, or use a ` +
                "signed delta (+3)"
              : error
                ? ERROR_COPY[error]
                : undefined;
          return (
            <li
              key={lever.nodeId}
              className='flex flex-col gap-1.5 border border-border bg-background/40 p-2'
              data-testid={`scenario-lever-${lever.nodeId}`}
            >
              <div className='flex items-start justify-between gap-2'>
                <div className='flex min-w-0 flex-col gap-0.5'>
                  <span className='truncate font-medium text-[12px]'>
                    {node ? measureTitle(node) : lever.nodeId}
                  </span>
                  {/* Baseline → the value this lever sets, and by how much.
                      The baseline alone reads as "nothing has been changed",
                      which is the opposite of what a pinned lever means. */}
                  <span data-testid={`scenario-lever-value-${lever.nodeId}`}>
                    <MeasureChange
                      baseline={data?.baseline}
                      simulated={data?.simulated}
                      delta={data?.delta}
                      format={formatNumber}
                      showDelta
                    />
                  </span>
                </div>
                <button
                  type='button'
                  onClick={() => unpin(lever.nodeId)}
                  className='shrink-0 text-muted-foreground hover:text-foreground'
                  aria-label={`Unpin ${node ? measureTitle(node) : lever.nodeId}`}
                >
                  <X size={12} />
                </button>
              </div>
              <LeverSlider
                raw={lever.raw}
                label={node ? measureTitle(node) : lever.nodeId}
                baseline={data?.baseline}
                onCommit={(next) => updateLeverRaw(lever.nodeId, next)}
              />
              <Input
                value={lever.raw}
                onChange={(e) => updateLeverRaw(lever.nodeId, e.target.value)}
                placeholder='11 · +5% · -3'
                className='h-7 font-mono text-xs'
              />
              {errorText && (
                <p className='text-[10px] text-destructive'>
                  <Backticked text={errorText} />
                </p>
              )}
            </li>
          );
        })}
      </ul>

      {/* The empty state says how to pin the first lever, then vanishes. A
          scenario is usually more than one lever, so the instruction has to
          outlive the empty state. */}
      {state.levers.length > 0 && (
        <p className='text-[10px] text-muted-foreground'>
          Click another measure in the graph to pin it as a lever too.
        </p>
      )}
    </div>
  );
}

/**
 * The percentage slider for one lever.
 *
 * Drag updates local state only; the scenario (and the URL, and propagation)
 * commit on release. Dragging fires continuously, and committing every frame
 * would rewrite the query string and re-run the prediction on every pixel.
 *
 * A `raw` the slider can't represent — an absolute target, or a signed delta
 * — parks the handle at zero and says so. The typed value still wins: the
 * text field is the source of truth, the slider is a fast way to write it.
 *
 * The slider writes ONLY percentages (that is what gives it a range without a
 * current value, see `leverPercent`), and a percentage is the one lever form
 * `resolveLever` cannot resolve without a baseline. So when this lever has no
 * current value to scale — a refused baseline, a delta-only scenario with no
 * time dimension — every position on it is an error, and it is disabled rather
 * than offered. Leaving it live was how the panel came to hand you a control
 * whose every output it then rejected, with copy underneath telling you to
 * type a signed delta instead.
 */
function LeverSlider({
  raw,
  label,
  baseline,
  onCommit
}: {
  raw: string;
  label: string;
  /** This lever's current value, or `undefined` if the baseline didn't get one. */
  baseline?: number;
  onCommit: (next: string) => void;
}) {
  const committed = percentFromRaw(raw);
  const [dragging, setDragging] = useState<number | null>(null);
  const percent = dragging ?? committed ?? 0;
  // 0 is as unusable as absent: a percentage of nothing is nothing, which is
  // `resolveLever`'s `zero_baseline` and not a move anyone asked for.
  const scalable = baseline !== undefined && baseline !== 0;

  const groupLabel = `${label} percentage change`;

  return (
    // The label belongs on the element carrying `role="slider"`, which is
    // Radix's THUMB — and `Slider` spreads its props onto the Root, so an
    // `aria-label` there names a generic div and the control itself stays
    // unnamed. `src/components/ui/shadcn/` is CLI-managed and must not be
    // hand-edited to add a thumb pass-through, so the name is carried by a
    // labelled group instead: a screen reader announces the group before the
    // slider inside it, which gets the name to the user. No `aria-label` on the
    // Slider itself — it would land on that same unroled Root and be ignored,
    // so it reads as a second mechanism where there is only one. Replace this
    // with a thumb-level label if the shadcn wrapper ever grows one upstream.
    <div className='flex flex-col gap-1' role='group' aria-label={groupLabel}>
      <Slider
        value={[percent]}
        min={-SLIDER_RANGE}
        max={SLIDER_RANGE}
        step={1}
        disabled={!scalable}
        onValueChange={(v) => setDragging(v[0])}
        onValueCommit={(v) => {
          setDragging(null);
          onCommit(rawFromPercent(v[0]));
        }}
      />
      <div className='flex items-baseline justify-between'>
        <span className='font-mono text-[10px] text-muted-foreground'>−{SLIDER_RANGE}%</span>
        <span className='font-mono text-foreground text-xs tabular-nums'>
          {committed === null && dragging === null ? "—" : rawFromPercent(percent)}
        </span>
        <span className='font-mono text-[10px] text-muted-foreground'>+{SLIDER_RANGE}%</span>
      </div>
      {!scalable ? (
        <p className='text-[10px] text-muted-foreground' data-testid='scenario-slider-unscalable'>
          {baseline === 0
            ? "This measure is 0 over the window, so a % can't move it — type a signed delta like +3."
            : "No current value to scale a % against, so the slider is off — type a signed delta like +3."}
        </p>
      ) : (
        committed === null &&
        raw.trim() !== "" && (
          <p className='text-[10px] text-muted-foreground'>
            <span className='font-mono'>{raw}</span> isn't a percentage, so the slider can't show it
            — drag to replace it, or keep typing.
          </p>
        )
      )}
    </div>
  );
}

/**
 * A lever whose measure no longer exists in the tree (branch switch,
 * `.view.yml` edit) is dropped rather than kept silently — a scenario built
 * on a dead lever is one nobody can reproduce. Returns the ids dropped so
 * far this mount, to render the one-line notice.
 */
function useDropVanishedLevers(
  tree: MetricTree,
  state: ScenarioState,
  onChange: (next: ScenarioUpdate) => void
): string[] {
  const [droppedIds, setDroppedIds] = useState<string[]>([]);

  useEffect(() => {
    const nodeIds = new Set(tree.nodes.map((n) => n.id));
    const vanished = state.levers.filter((l) => !nodeIds.has(l.nodeId));
    if (vanished.length === 0) return;
    setDroppedIds((prev) => [...prev, ...vanished.map((l) => l.nodeId)]);
    // An updater, not a spread of this render's `state`: a co-firing
    // time-dimension adopt or lever pin would otherwise be reverted by
    // whichever of the two committed second.
    onChange((prev) => ({
      ...prev,
      levers: prev.levers.filter((l) => nodeIds.has(l.nodeId))
    }));
  }, [tree, state, onChange]);

  return droppedIds;
}
