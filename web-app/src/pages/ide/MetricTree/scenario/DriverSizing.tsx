import { ChevronDown } from "lucide-react";
import { useState } from "react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { cn } from "@/libs/shadcn/utils";
import type { FittedDriver } from "@/types/metricTree";
import { formatNumber, shortMeasureName } from "@/utils/measureFormat";
import { curvatureNote } from "./curvatureNote";
import { formatLeverPercent } from "./leverPercent";
import { readResponse } from "./readResponse";

interface DriverSizingProps {
  /** Edges the baseline measured a coefficient for. */
  fits: FittedDriver[];
  /** Edges it declined to size, each carrying its reason. */
  refusals: FittedDriver[];
}

/**
 * What the baseline made of each driver edge, behind one line.
 *
 * Every sentence in here is worth reading once and almost never twice, and
 * there is one per edge: a tree with a dozen undeclared drivers pushed the
 * levers themselves — the controls the panel exists for — off the bottom of a
 * narrow column. So the findings collapse to a count, and the count states
 * both halves, because "6 not sized" is the reason a branch of the canvas
 * shows nothing and has to be visible without opening anything.
 *
 * Collapsed by default including when everything fitted: an all-fitted tree is
 * the case with the least to say, and a section that opens itself whenever it
 * has content is not a disclosure.
 */
export function DriverSizing({ fits, refusals }: DriverSizingProps) {
  const [open, setOpen] = useState(false);
  const total = fits.length + refusals.length;
  if (total === 0) return null;

  const summary = [
    fits.length > 0 ? `${fits.length} sized from history` : undefined,
    refusals.length > 0 ? `${refusals.length} not sized` : undefined
  ]
    .filter((part) => part !== undefined)
    .join(" · ");

  return (
    <Collapsible open={open} onOpenChange={setOpen} data-testid='scenario-driver-sizing'>
      <CollapsibleTrigger
        className='flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground'
        data-testid='scenario-driver-sizing-toggle'
      >
        <ChevronDown
          className={cn("h-3 w-3 shrink-0 transition-transform", !open && "-rotate-90")}
        />
        Driver edges · {summary}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className='mt-1.5 flex flex-col gap-1.5'>
          {fits.length > 0 && (
            <div className='flex flex-col gap-1' data-testid='scenario-fitted-coefficients'>
              {fits.map((f) => (
                <FittedResponse key={`${f.from}->${f.to}`} fit={f} />
              ))}
            </div>
          )}

          {refusals.map((f) => (
            <RefusedFit key={`${f.from}->${f.to}`} fit={f} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

/** One edge the baseline declined to size, and what it saw before declining. */
function RefusedFit({ fit }: { fit: FittedDriver }) {
  return (
    <p
      className='text-[11px] text-muted-foreground leading-relaxed'
      data-testid='scenario-fit-refused'
    >
      <span className='font-medium text-foreground'>
        {shortMeasureName(fit.from)} → {shortMeasureName(fit.to)}
      </span>{" "}
      could not be sized from history: {fit.refusal}. It stays a direction without a magnitude, so
      nothing is forecast across it.
      {/* What the fit actually saw. A refusal that names a statistical
          cause ("the driver does not vary") but not its sample is
          unfalsifiable from the screen: a collapsed panel and a genuinely
          flat driver read identically, and telling them apart otherwise
          means reproducing the query by hand. */}
      {fit.n ? (
        <>
          {" "}
          Measured over {fit.n.toLocaleString()} paired observation
          {fit.n === 1 ? "" : "s"}
          {fit.n_panels
            ? ` across ${fit.n_panels.toLocaleString()} panel${fit.n_panels === 1 ? "" : "s"}`
            : ""}
          .
        </>
      ) : null}
    </p>
  );
}

/**
 * One driver response the baseline measured from history.
 *
 * Deliberately says nothing about the SHAPE. There is no per-form wording and no
 * coefficient unit, because both had to grow a case per shape and neither is what
 * a reader wants: "+10% buys +1,838 orders" is stated in the measure's own terms,
 * where "0.854 per unit" needs a different sentence depending on the form and
 * still leaves the reader converting. Everything here is read off the sampled
 * response (`readResponse`), so a shape added to the engine needs no change.
 */
function FittedResponse({ fit }: { fit: FittedDriver }) {
  const from = shortMeasureName(fit.from);
  const to = shortMeasureName(fit.to);
  const r = readResponse(fit);
  const measured =
    fit.form_source === "inferred"
      ? "shape and size both measured over the baseline window"
      : "size measured over the baseline window";

  return (
    <p className='text-[11px] text-muted-foreground leading-relaxed'>
      <span className='font-medium text-foreground'>
        {from} → {to}
      </span>{" "}
      fitted from history — {measured}
      {fit.n ? ` (n=${fit.n})` : ""}.
      {r.samples.length > 0 ? (
        <>
          {" "}
          {r.samples
            .map((s) => `${formatLeverPercent(s.lever)} → ${formatNumber(s.delta)}`)
            .join(", ")}
          .
        </>
      ) : null}
      {r.peak !== undefined ? (
        // The one claim here with a ceiling in it. Reported only when the best
        // outcome is an INTERIOR peak — a maximum at the edge of the sampled range
        // is just the largest lever evaluated, not a recommendation.
        <>
          {" "}
          <span className='font-medium text-foreground'>
            Best around {formatLeverPercent(r.peak)}
          </span>
          {r.breakEven !== undefined
            ? `, and past ${formatLeverPercent(r.breakEven)} it lowers ${to} instead of raising it.`
            : "."}
          {curvatureNote(fit)}
        </>
      ) : r.declining ? (
        // The opposite finding from saturation, and the one that used to be
        // reported AS saturation: pushing this lever does not buy less each
        // time, it costs. No `breakEven` here — `declining` means every
        // sampled delta is already <= 0, so there is no crossing to name.
        <>
          {" "}
          <span className='font-medium text-foreground'>
            Pushing this lowers {to} across the whole sampled range.
          </span>
        </>
      ) : r.saturating ? (
        <> Each further increase buys less than the last.</>
      ) : r.breakEven !== undefined ? (
        // The arm that actually rescues `breakEven`: a curve that rises, then
        // crosses. It has no interior peak to hang the crossing off, so before
        // this it was computed and never rendered.
        //
        // Carries the curvature qualifier too. A crossing is as sensitive to a
        // marginal curvature as a turn is — both are read off the same fitted
        // shape — so scoping the caveat to the peak would have been an accident
        // of which arm was written first rather than a claim about the number.
        <>
          {" "}
          Past {formatLeverPercent(r.breakEven)} it lowers {to} instead of raising it.
          {curvatureNote(fit)}
        </>
      ) : null}
      {fit.n_nonpositive ? (
        // Only ever non-zero where a log was involved, and it lowers `n`, which is
        // what the refusal gate reads.
        <> {fit.n_nonpositive} day(s) were left out for having a non-positive value.</>
      ) : null}
    </p>
  );
}
