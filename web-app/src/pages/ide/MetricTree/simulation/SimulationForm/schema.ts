import type {
  MechanismSpecInput,
  SimulationSpecInput,
  SimulationSummary
} from "@/types/simulation";
import { DEFAULT_BUDGET_JITTER_SD, DEFAULT_LEVER, RESERVED_COLUMN_NAMES } from "@/types/simulation";

/**
 * The form's own value type. Identical to `SimulationSpecInput` except
 * `declared_lag_days`, which the form keeps as raw input text (`""` while
 * blank) rather than a number — a number input's registered value is only a
 * number via RHF's `valueAsNumber`, which turns an empty field into `NaN`
 * rather than the "customer guessed right" blank this field means to allow.
 */
export type SimulationFormValues = Omit<SimulationSpecInput, "mechanism"> & {
  mechanism: Omit<MechanismSpecInput, "declared_lag_days"> & {
    declared_lag_days: string;
  };
};

/**
 * A known-good starting point — the same `calibrate` numbers as the two
 * declared worlds already in `simulations/`, so a first-time author gets a
 * world that validates immediately and can be tuned from there, rather than
 * a blank form that fails on `margin` or an unsolvable curve before they've
 * touched anything domain-specific.
 */
export function defaultNewWorldValues(): SimulationFormValues {
  return {
    name: "",
    description: "",
    seed: 1,
    replicates: 1,
    periods: 30,
    period_days: 7,
    history_days: 180,
    start_date: new Date().toISOString().slice(0, 10),
    entities: {
      count: 24,
      scale_sigma: 0.4
    },
    baseline: {
      sales_per_entity_day: 1500,
      margin: 0.36,
      demand_shock_rho: 0.5,
      demand_shock_sd: 0.08,
      weekly_seasonality: 0.15,
      budget_jitter_sd: DEFAULT_BUDGET_JITTER_SD
    },
    mechanism: {
      driver: "marketing_spend",
      target: "net_sales",
      lag_days: 7,
      declared_lag_days: "",
      noise_ratio: 0.05,
      calibrate: {
        anchor_spend_share: 0.02,
        local_slope_at_anchor: 4.0,
        optimum_at: 3.0
      }
    },
    lever: { ...DEFAULT_LEVER }
  };
}

/**
 * Loads an existing declared world into the form. `definition` is whatever
 * the compile boundary (or FS fallback) handed back — the same JSON shape
 * `SimulationSpec` deserializes, so most fields carry over verbatim. The
 * optional/defaulted ones (`replicates`, `baseline.budget_jitter_sd`,
 * `lever`, `description`) are backfilled here because an older file that
 * predates one of those fields never wrote it, and the form has to show the
 * value the world actually runs with — not a blank.
 */
export function valuesFromSummary(world: SimulationSummary): SimulationFormValues {
  const d = world.definition as Partial<SimulationSpecInput>;
  const baseline = d.baseline as SimulationSpecInput["baseline"] | undefined;
  const mechanism = d.mechanism as SimulationSpecInput["mechanism"] | undefined;
  const calibrate = mechanism?.calibrate;

  return {
    name: d.name ?? world.name,
    description: d.description ?? "",
    seed: d.seed ?? 1,
    replicates: d.replicates ?? 1,
    periods: d.periods ?? 1,
    period_days: d.period_days ?? 1,
    history_days: d.history_days ?? 1,
    start_date: d.start_date ?? new Date().toISOString().slice(0, 10),
    entities: {
      count: d.entities?.count ?? 1,
      scale_sigma: d.entities?.scale_sigma ?? 0
    },
    baseline: {
      sales_per_entity_day: baseline?.sales_per_entity_day ?? 0,
      margin: baseline?.margin ?? 0,
      demand_shock_rho: baseline?.demand_shock_rho ?? 0,
      demand_shock_sd: baseline?.demand_shock_sd ?? 0,
      weekly_seasonality: baseline?.weekly_seasonality ?? 0,
      budget_jitter_sd: baseline?.budget_jitter_sd ?? DEFAULT_BUDGET_JITTER_SD
    },
    mechanism: {
      driver: mechanism?.driver ?? "",
      target: mechanism?.target ?? "",
      lag_days: mechanism?.lag_days ?? 1,
      declared_lag_days: mechanism?.declared_lag_days?.toString() ?? "",
      noise_ratio: mechanism?.noise_ratio ?? 0,
      calibrate: {
        anchor_spend_share: calibrate?.anchor_spend_share ?? 0,
        local_slope_at_anchor: calibrate?.local_slope_at_anchor ?? 0,
        optimum_at: calibrate?.optimum_at ?? 0
      }
    },
    lever: {
      min_multiple: d.lever?.min_multiple ?? DEFAULT_LEVER.min_multiple,
      max_multiple: d.lever?.max_multiple ?? DEFAULT_LEVER.max_multiple,
      max_move_per_period: d.lever?.max_move_per_period ?? DEFAULT_LEVER.max_move_per_period,
      explore_jitter_sd: d.lever?.explore_jitter_sd ?? DEFAULT_LEVER.explore_jitter_sd
    }
  };
}

/**
 * Form values → the JSON body `POST /simulations/validate` checks and the
 * object serialized into the `.simulation.yml`. Drops `description` when
 * blank and `declared_lag_days` when empty, rather than posting `""` for a
 * field the backend expects as a string or an optional integer — an empty
 * declared lag means "the customer guessed right", not "guessed zero".
 */
export function toSpecInput(values: SimulationFormValues): SimulationSpecInput {
  const { description, mechanism, ...rest } = values;
  const { declared_lag_days, ...mechanismRest } = mechanism;
  const declaredLag = declared_lag_days.trim();

  return {
    ...rest,
    ...(description?.trim() ? { description: description.trim() } : {}),
    mechanism: {
      ...mechanismRest,
      ...(declaredLag === "" ? {} : { declared_lag_days: Number(declaredLag) })
    }
  };
}

/**
 * A bare column name: a letter or underscore, then letters, digits or
 * underscores.
 *
 * Mirrors `is_bare_identifier` in `crates/simulation/src/spec.rs`, which
 * `SimulationSpec::validate` — the authority for this rule — applies to
 * `mechanism.driver`/`target`. Those strings are interpolated raw into the
 * generated CSV header and into the generated view YAML (a measure `name:`,
 * its `expr:` and a `drivers.measure` path), so a comma, colon, space, quote,
 * newline, hyphen, dot or leading digit/`#` only fails three layers down. The
 * copy exists purely so the author sees it inline instead of as a server error.
 */
export const BARE_COLUMN_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * The client mirror of the `driver`/`target` checks in
 * `SimulationSpec::validate` (`crates/simulation/src/spec.rs`) — identifier
 * class, then reserved columns, then the driver ≠ target rule, in that order.
 * Returns `null` when the name passes. Messages track the Rust ones so the
 * same rule never gets two explanations.
 */
export function columnNameError(value: string, otherColumnName: string): string | null {
  if (!BARE_COLUMN_NAME_PATTERN.test(value)) {
    return "must be a bare column name: a letter or underscore, then letters, digits or underscores";
  }
  if ((RESERVED_COLUMN_NAMES as readonly string[]).includes(value)) {
    return `'${value}' is already declared by every generated world (${RESERVED_COLUMN_NAMES.join(", ")})`;
  }
  if (value === otherColumnName) {
    return "driver and target must be different columns";
  }
  return null;
}

/**
 * The curated `driver` names a world may be built from.
 *
 * These fields *name* the mechanism's columns; they do not choose it. Every
 * generated world runs the one mechanism `crates/simulation/src/world.rs`
 * implements — a lagged spend with diminishing returns lifting a revenue
 * level, scored as `target − prime_cost − driver` — so the driver has to read
 * as money spent and the target as the revenue it buys, in the same units, or
 * the world's own profit objective stops meaning anything. That is why these
 * are a list rather than a free-text box. The backend still accepts any bare
 * column name (`SimulationSpec::validate`), which the form's "Custom…" option
 * keeps reachable.
 */
export const DRIVER_COLUMN_NAMES = [
  "marketing_spend",
  "ad_spend",
  "promo_spend",
  "trade_spend"
] as const;

/** The curated `target` names — see `DRIVER_COLUMN_NAMES`. */
export const TARGET_COLUMN_NAMES = ["net_sales", "gross_sales", "revenue", "bookings"] as const;
