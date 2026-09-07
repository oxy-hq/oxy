import type { FittedDriver, MetricTree } from "@/types/metricTree";
import { formatNumber, shortMeasureName } from "@/utils/measureFormat";
import { FORM_HELP, MetaBadge } from "../../components/semanticGraph";
import { confidenceHelp } from "./ConfidenceMark";
import { MeasureChange } from "./MeasureChange";
import type { ScenarioNodeData } from "./nodeValue";
import { buildTrace, countPathsTo, type TraceHop, unsizableHop } from "./propagationTrace";

interface ImpactDetailProps {
  data: ScenarioNodeData;
  /** Where every hop's metadata comes from. Optional: without it the route is
   *  still named, just unannotated — better than hiding the route. */
  tree?: Pick<MetricTree, "edges">;
  /** The baseline's measured coefficients, so a hop can say whether its number
   *  was declared in YAML or regressed from history. */
  fitted?: FittedDriver[];
  /** The pinned levers, needed only to tell a single-route impact from one
   *  whose total sums several. */
  leverIds?: string[];
}

/**
 * Why one downstream measure moved by the amount shown.
 *
 * The list answers "what changed"; this answers "on what basis". Everything
 * here is already in the panel's hands — `predict` returns the route, the tree
 * carries each edge, the baseline carries what it fitted — so an impact
 * explains itself with no extra request.
 *
 * The two things it must not do, both of which the raw response invites:
 * present the returned route as the sole cause of a number that sums several,
 * and print an `unquantifiable` impact's `form`, which is a placeholder rather
 * than a fitted shape.
 */
export function ImpactDetail({ data, tree, fitted, leverIds }: ImpactDetailProps) {
  const hops = buildTrace(data.path, tree, fitted);
  const { count: pathCount, capped } = countPathsTo(data.node.id, leverIds ?? [], tree, fitted);
  // Past the cap the walk stopped early, so the count is a floor. "8+" is the
  // only honest rendering — printing it flat would report a measure reached by
  // forty routes as reached by eight.
  const pathCountLabel = capped ? `${pathCount}+` : `${pathCount}`;
  const unsized = data.state === "unquantifiable" ? unsizableHop(hops) : undefined;

  return (
    <div
      className='flex flex-col gap-2 border-border border-l-2 bg-muted/30 px-3 py-2'
      data-testid={`scenario-impact-detail-${data.node.id}`}
    >
      {/* The move at panel width. The row above shows the same figures compactly;
          here there is room for the absolute change alongside the percentage. */}
      {data.state !== "unquantifiable" && (
        <MeasureChange
          baseline={data.baseline}
          simulated={data.simulated}
          delta={data.delta}
          format={formatNumber}
          showDelta
        />
      )}

      {hops.length > 0 ? (
        <div className='flex flex-col gap-1.5'>
          <span className='text-[9.5px] text-muted-foreground uppercase tracking-wider'>
            {pathCount > 1 ? `Route (1 of ${pathCountLabel})` : "Route"}
          </span>
          {hops.map((hop) => (
            <Hop key={`${hop.from}->${hop.to}`} hop={hop} />
          ))}
        </div>
      ) : (
        // No route means a predict result that predates this tree, or a
        // baseline-only state. Saying nothing here would read as "one hop".
        <p className='text-[11px] text-muted-foreground leading-relaxed'>
          The route this change took isn't available — the metric tree has changed since this
          simulation ran. Re-pin the lever to trace it again.
        </p>
      )}

      {/* Lag is cumulative over the whole path, so it belongs to the impact
          rather than to any one hop. */}
      {data.lag ? (
        <p className='text-[11px] text-muted-foreground leading-relaxed'>
          Modelled to land about {data.lag} day{data.lag === 1 ? "" : "s"} after the change, summed
          over the route.
        </p>
      ) : null}

      {pathCount > 1 && (
        <p
          className='text-[11px] text-muted-foreground leading-relaxed'
          data-testid={`scenario-impact-multipath-${data.node.id}`}
        >
          {pathCountLabel} routes reach this measure and carry a magnitude. The figure above sums
          all of them; the route shown is one.
        </p>
      )}

      {data.confidence && (
        <p className='text-[11px] text-muted-foreground leading-relaxed'>
          {confidenceHelp(data.confidence)}
        </p>
      )}

      {/* An unquantifiable verdict is only actionable if it names the edge that
          caused it. When the shown route is fully sized the break is on a route
          `predict` didn't return, and pointing at a hop here would be a guess. */}
      {data.state === "unquantifiable" &&
        (unsized ? (
          <p className='text-[11px] text-muted-foreground leading-relaxed'>
            <span className='font-medium text-foreground'>
              {shortMeasureName(unsized.from)} → {shortMeasureName(unsized.to)}
            </span>{" "}
            has no magnitude
            {unsized.fit?.refusal ? `: ${unsized.fit.refusal}` : ""}. Declare a{" "}
            <span className='font-mono'>coefficient:</span> on it to size this impact.
          </p>
        ) : hops.length > 0 ? (
          <p className='text-[11px] text-muted-foreground leading-relaxed'>
            Every hop on the route shown is sized, so the unsizable edge is on another route into
            this measure.
          </p>
        ) : null)}
    </div>
  );
}

/** One edge of the route, with whatever the model knows about it. */
function Hop({ hop }: { hop: TraceHop }) {
  return (
    <div className='flex flex-col gap-0.5'>
      <div className='flex flex-wrap items-center gap-1.5'>
        <span className='font-mono text-[11px] text-foreground'>
          {shortMeasureName(hop.from)} → {shortMeasureName(hop.to)}
        </span>
        {hop.kind && (
          <MetaBadge
            tooltip={
              hop.kind === "component"
                ? "A component of a composite measure: the change carries through as arithmetic, not a forecast."
                : "A modelled driver relationship: the change is scaled by a coefficient, so the result is an estimate."
            }
          >
            {hop.kind}
          </MetaBadge>
        )}
        {hop.sign !== undefined && hop.sign < 0 && (
          <MetaBadge tooltip='This component subtracts from its parent, so the change arrives with its sign flipped.'>
            −
          </MetaBadge>
        )}
        {hop.form && <MetaBadge tooltip={FORM_HELP[hop.form]}>{hop.form}</MetaBadge>}
        {hop.form && hop.formDeclared === false && (
          <MetaBadge tooltip={inferredHelp(hop)}>inferred</MetaBadge>
        )}
        {hop.lag ? (
          <MetaBadge tooltip='Days before this hop’s effect appears.'>{hop.lag}d</MetaBadge>
        ) : null}
      </div>
      <CoefficientLine hop={hop} />
      {hop.description && (
        <span className='text-[10.5px] text-muted-foreground leading-relaxed'>
          {hop.description}
        </span>
      )}
    </div>
  );
}

const INFERRED_BASE =
  "The shape was measured from history rather than declared, so it can change as the window moves.";

/** The engine's parsimony band, from `driver-response-standardization.md`:
 *  candidates within this many AIC of the best are a statistical dead heat and
 *  the FEWEST-TERM shape takes it. Mirrored here only to word the tooltip —
 *  nothing in the panel re-runs the selection. */
const AIC_DEAD_HEAT = 10;

/**
 * Why an inferred shape won, when the fit says.
 *
 * `candidates` scores every shape the engine considered on one comparable
 * scale, and the type's own note is the reason to render it: "a curve beat a
 * line by 945" is checkable, "the engine chose quadratic" is not. A candidate
 * that failed the significance gate was never eligible however good its score,
 * so a rival is always the best ELIGIBLE one — naming an ineligible shape as
 * the thing that was beaten would misdescribe the choice.
 *
 * ANCHOR ON `hop.form`, NOT ON THE LOWEST SCORE. Lowest-AIC-wins is not the
 * rule: within `AIC_DEAD_HEAT` the fewest-term shape takes it, because a richer
 * nested shape can reproduce a simpler one exactly and then spend its extra
 * term on noise. On this project's own fixture `cubic` scores 2.2 better than
 * the chosen `quadratic`, so treating the best score as the winner produced
 * "It beat linear-log-quadratic by 0.60 AIC" beside a badge reading
 * `quadratic` — two shapes named, neither of them the one on screen. The shape
 * that is actually displayed is the one the sentence has to be about.
 */
export function inferredHelp(hop: TraceHop): string {
  const eligible = (hop.fit?.candidates ?? [])
    .filter((c) => c.all_terms_significant)
    .sort((a, b) => a.aic - b.aic);
  if (eligible.length < 2) return INFERRED_BASE;

  const chosen = hop.form ? eligible.find((c) => c.form === hop.form) : undefined;
  // The displayed shape is not among the eligible candidates — a declared form,
  // or a payload the panel cannot reconcile. Saying nothing beats guessing
  // which of these scores belongs to the badge above.
  if (!chosen) return INFERRED_BASE;

  const best = eligible[0];
  const rival = chosen === best ? eligible[1] : best;
  const margin = Math.abs(chosen.aic - rival.aic);
  if (!Number.isFinite(margin)) return INFERRED_BASE;

  const outOf = `out of ${eligible.length} shapes that cleared the significance gate`;
  if (chosen === best) {
    return `${INFERRED_BASE} It beat ${rival.form} by ${formatNumber(margin)} AIC, ${outOf}.`;
  }
  // The chosen shape scored WORSE than the best, which under the documented
  // rule can only be the dead-heat case. Check it rather than assume it: the
  // whole point of anchoring on `hop.form` was to stop the panel re-deriving
  // the engine's selection, and a payload that breaks the rule would otherwise
  // print a number above 10 inside a sentence asserting it is under 10.
  if (margin > AIC_DEAD_HEAT) return INFERRED_BASE;
  return `${INFERRED_BASE} It scored within ${formatNumber(margin)} AIC of ${rival.form} — inside the ${AIC_DEAD_HEAT}-point band the engine treats as a dead heat, where the simpler shape takes it — ${outOf}.`;
}

/**
 * What the hop multiplies by, and on whose authority.
 *
 * Declared and fitted are not the same claim — one is an assertion in a file,
 * the other a regression over the baseline window that moves when the window
 * does — so the number is never shown without its provenance.
 */
function CoefficientLine({ hop }: { hop: TraceHop }) {
  if (hop.coefficientSource === "declared") {
    return (
      <span className='font-mono text-[10.5px] text-muted-foreground'>
        coefficient {formatNumber(hop.coefficient ?? 0)} · declared
      </span>
    );
  }

  if (hop.coefficientSource === "fitted") {
    const n = hop.fit?.n;
    // `t` and the observed spread are the two things a reader needs to weigh a
    // measured number that `n` alone does not give: how far the estimate is
    // from zero in its own standard errors, and the range of driver values it
    // was measured over — outside which propagation refuses rather than
    // extrapolates, so the range is a limit on the answer, not trivia.
    const t = hop.fit?.t_stat;
    const domain = hop.fit?.domain;
    // Built as a list rather than as three interleaved ternaries around a
    // hand-written "(" and ")": that shape made `t` depend on `n` being present
    // to get its parentheses, so a fit carrying a t-statistic and no `n` — which
    // the wire type permits, both being optional — dropped the t silently.
    // `!= null`, per `FittedDriver.coefficient`'s contract three fields above
    // `n`: these are `Option<f64>` on a GIT-PINNED struct, so `skip_serializing_if`
    // is a serde attribute today rather than a guarantee, and every read has to
    // accept both encodings. Strict `=== undefined` would let a wire `null`
    // through to `null.toLocaleString()` — a TypeError in the render phase,
    // which on a panel with no error boundary takes the page with it.
    const stats = [
      n == null ? undefined : `n=${n.toLocaleString()}`,
      t == null || !Number.isFinite(t) ? undefined : `t=${formatNumber(t)}`
    ].filter((part): part is string => part !== undefined);
    return (
      <span className='font-mono text-[10.5px] text-muted-foreground'>
        coefficient {formatNumber(hop.coefficient ?? 0)} · fitted from history
        {stats.length > 0 ? ` (${stats.join(", ")})` : ""}
        {domain && domain.length === 2
          ? ` · measured over ${formatNumber(domain[0])}–${formatNumber(domain[1])}`
          : ""}
      </span>
    );
  }

  if (hop.coefficientSource === "refused") {
    return (
      <span className='text-[10.5px] text-muted-foreground leading-relaxed'>
        Not sized from history{hop.fit?.refusal ? `: ${hop.fit.refusal}` : ""}
      </span>
    );
  }

  // A driver edge with neither a declared coefficient nor a fit is qualitative:
  // the model knows the relationship exists and not how big it is. Silence here
  // reads as "sized, magnitude omitted", which is the opposite.
  if (hop.kind === "driver") {
    return (
      <span className='text-[10.5px] text-muted-foreground leading-relaxed'>
        No magnitude modelled — a direction only.
      </span>
    );
  }

  // A component edge's quantitative content is its sign, already badged above,
  // so there is nothing further to state.
  return null;
}
