//! The world engine: mechanisms run forward, one day at a time.
//!
//! Everything here is built in the direction the `.view.yml` will claim —
//! spend is an *input*, sales *read* it. That is the whole difference between a
//! world and a fixture, and it is what makes an intervention meaningful: change
//! the spend a policy chooses and different sales genuinely follow.

use std::collections::VecDeque;

use chrono::{Datelike, NaiveDate};

use crate::SimulationError;
use crate::rng::Rng;
use crate::spec::{ResponseCurve, SimulationSpec};

/// One row at the grain the semantic layer will read: entity × business date.
///
/// Field names here are internal Rust identifiers only — this mechanism
/// (a driver lifts a target after a lag, cost is a margin share of the
/// target) is generic, not specific to marketing or sales. What a world
/// actually *calls* these two values is `spec.mechanism.driver`/`.target`,
/// and that is what ends up in the generated CSV header and `.view.yml`
/// (`world_dir::csv_header`, `world_dir::view_yml`) — never these field
/// names, which stay fixed regardless of what the declared world names them.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityDay {
    pub entity_id: u32,
    pub date: NaiveDate,
    pub net_sales: f64,
    pub marketing_spend: f64,
    pub prime_cost: f64,
}

impl EntityDay {
    /// `net_sales − prime_cost − marketing_spend`, the objective the policies race on.
    pub fn profit(&self) -> f64 {
        self.net_sales - self.prime_cost - self.marketing_spend
    }
}

pub struct World {
    spec: SimulationSpec,
    curve: ResponseCurve,
    /// Per-entity size multiplier. This is the between-entity contrast that
    /// within-panel demeaning exists to remove.
    scale: Vec<f64>,
    /// AR(1) latent demand, per entity. Never emitted — it is the confounder.
    shock: Vec<f64>,
    /// Spend awaiting its lag, per entity. Front is the oldest.
    pending_spend: Vec<VecDeque<f64>>,
    /// Trailing sales per entity, for the legacy spend rule.
    trailing: Vec<VecDeque<f64>>,
    shock_rng: Rng,
    noise_rng: Rng,
    jitter_rng: Rng,
    day: u32,
}

/// Days of sales the legacy rule averages over when setting a budget.
pub const TRAILING_WINDOW: usize = 7;

/// The legacy budget rule: a share of trailing sales, jittered.
///
/// One definition, called from two places — the burn-in that produces a
/// customer's opening history, and the `legacy` policy that keeps running it
/// through the loop. If those two ever disagreed, the `legacy` arm of a profit
/// race would be scored against a history no policy in the race produced.
pub fn legacy_budget(share: f64, trailing_mean_sales: f64, jitter: f64) -> f64 {
    (share * trailing_mean_sales * jitter.max(0.1)).max(0.0)
}

/// Mean daily sales per entity over the last `window` days present in `rows`.
///
/// Computed from emitted rows rather than read off the world, because the
/// legacy rule is something a *customer* runs: it may only ever see what the
/// warehouse holds. A policy handed the world's own trailing state would be
/// reaching past the rows, which is the one thing this crate exists to prevent.
pub fn trailing_mean_sales(rows: &[EntityDay], entity_count: usize, window: usize) -> Vec<f64> {
    let mut recent: Vec<VecDeque<f64>> = vec![VecDeque::new(); entity_count];
    let mut sorted: Vec<&EntityDay> = rows.iter().collect();
    sorted.sort_by_key(|r| r.date);
    for row in sorted {
        let Some(slot) = recent.get_mut(row.entity_id as usize) else {
            continue;
        };
        slot.push_back(row.net_sales);
        if slot.len() > window {
            slot.pop_front();
        }
    }
    recent.iter().map(mean).collect()
}

impl World {
    pub fn new(spec: SimulationSpec) -> Result<Self, SimulationError> {
        let curve = spec.curve()?;
        let count = spec.entities.count as usize;

        // One labelled stream per concern, so adding a mechanism later cannot
        // shift the draws of the ones already recorded.
        let mut scale_rng = Rng::stream(spec.seed, "entity_scale");
        let scale: Vec<f64> = (0..count)
            .map(|_| scale_rng.lognormal(spec.entities.scale_sigma))
            .collect();

        let lag = spec.mechanism.lag_days as usize;
        let pending_spend = vec![VecDeque::from(vec![curve.anchor_spend; lag]); count];
        let trailing =
            vec![VecDeque::from(vec![spec.baseline.sales_per_entity_day; TRAILING_WINDOW]); count];

        Ok(Self {
            shock_rng: Rng::stream(spec.seed, "demand_shock"),
            noise_rng: Rng::stream(spec.seed, "target_noise"),
            jitter_rng: Rng::stream(spec.seed, "legacy_jitter"),
            curve,
            scale,
            shock: vec![0.0; count],
            pending_spend,
            trailing,
            day: 0,
            spec,
        })
    }

    /// The solved truth. **Scorer only** — a policy that reads this is not being
    /// measured, it is being told the answer.
    pub fn truth(&self) -> ResponseCurve {
        self.curve
    }

    pub fn spec(&self) -> &SimulationSpec {
        &self.spec
    }

    pub fn entity_count(&self) -> usize {
        self.scale.len()
    }

    /// Generate the history a customer already has when we arrive, under the
    /// legacy rule (budget as a share of trailing sales).
    ///
    /// Not an implementation shortcut: it is the realistic opening condition, and
    /// it is what puts confounding in the data from period 1 rather than only
    /// once a policy introduces it. A burn-in at a *flat* spend would instead
    /// open with no within-panel variation at all, and every run would begin with
    /// a refusal that says "unidentified" when it only means "nothing has moved
    /// yet".
    pub fn warm_up(&mut self) -> Vec<EntityDay> {
        let days = self.spec.history_days;
        let mut rows = Vec::with_capacity(days as usize * self.entity_count());
        for _ in 0..days {
            let spend = self.legacy_spend();
            rows.extend(self.generate_day(&spend));
        }
        rows
    }

    /// Advance one decision period at the given per-entity spend, held for every
    /// day of the period.
    pub fn step(&mut self, spend_per_entity: &[f64]) -> Vec<EntityDay> {
        debug_assert_eq!(spend_per_entity.len(), self.entity_count());
        let mut rows = Vec::with_capacity(self.spec.period_days as usize * self.entity_count());
        for _ in 0..self.spec.period_days {
            rows.extend(self.generate_day(spend_per_entity));
        }
        rows
    }

    /// What the legacy rule would spend next, given what each entity has been
    /// selling. Exposed so the `legacy` policy and the burn-in share one
    /// definition rather than drifting apart.
    pub fn legacy_spend(&mut self) -> Vec<f64> {
        let share = self.spec.mechanism.calibrate.anchor_spend_share;
        // The declared spread, not a constant: this is the *identification* axis
        // — the only movement in the regressor that is not the confounder — and
        // a grid that cannot sweep it cannot draw the left-hand column of the
        // outcome map, where the gate is what saves you.
        let jitter_sd = self.spec.baseline.budget_jitter_sd;
        (0..self.entity_count())
            .map(|e| {
                let jitter = self.jitter_rng.gauss(1.0, jitter_sd);
                legacy_budget(share, mean(&self.trailing[e]), jitter)
            })
            .collect()
    }

    /// The opening spend the calibration is anchored at, per entity-day.
    pub fn anchor_spend(&self) -> f64 {
        self.curve.anchor_spend
    }

    fn generate_day(&mut self, spend_per_entity: &[f64]) -> Vec<EntityDay> {
        let date = self.spec.start_date + chrono::Duration::days(self.day as i64);
        let seasonal = self.seasonal_factor(date);
        let margin = self.spec.baseline.margin;
        let base_level = self.spec.baseline.sales_per_entity_day;

        let mut rows = Vec::with_capacity(self.entity_count());
        // Not a range-loop that wants iterating: `e` indexes four parallel
        // collections (three of them `self`'s, mutably), and the entity count is
        // the world's, not the argument's.
        #[allow(clippy::needless_range_loop)]
        for e in 0..self.entity_count() {
            self.advance_shock(e);

            // The spend that lands today was chosen `lag` days ago. Push today's
            // choice to the back and take the matured one off the front, so the
            // lag is carried by the data rather than by an index the caller has
            // to get right.
            self.pending_spend[e].push_back(spend_per_entity[e]);
            let matured = self.pending_spend[e].pop_front().unwrap_or(0.0);

            let base = base_level * self.scale[e] * seasonal * self.shock[e].exp();
            let noise = self
                .noise_rng
                .gauss(0.0, self.spec.mechanism.noise_ratio * base_level);

            // The response is homogeneous across entities on purpose: it makes
            // β_true a single number the fit can be scored against. Heterogeneous
            // effects are a grid axis for later, not a Phase 1 complication.
            let net_sales = (base + self.curve.response(matured) + noise).max(0.0);

            self.trailing[e].push_back(net_sales);
            if self.trailing[e].len() > TRAILING_WINDOW {
                self.trailing[e].pop_front();
            }

            rows.push(EntityDay {
                entity_id: e as u32,
                date,
                net_sales,
                marketing_spend: spend_per_entity[e],
                prime_cost: (1.0 - margin) * net_sales,
            });
        }
        self.day += 1;
        rows
    }

    fn advance_shock(&mut self, entity: usize) {
        let rho = self.spec.baseline.demand_shock_rho;
        let sd = self.spec.baseline.demand_shock_sd;
        self.shock[entity] = rho * self.shock[entity] + self.shock_rng.gauss(0.0, sd);
    }

    fn seasonal_factor(&self, date: NaiveDate) -> f64 {
        let dow = date.weekday().num_days_from_monday() as f64;
        1.0 + self.spec.baseline.weekly_seasonality * (std::f64::consts::TAU * dow / 7.0).cos()
    }
}

fn mean(values: &VecDeque<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Total realized profit over a set of rows.
pub fn total_profit(rows: &[EntityDay]) -> f64 {
    rows.iter().map(EntityDay::profit).sum()
}

#[cfg(test)]
mod tests;
