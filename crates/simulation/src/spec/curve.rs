//! The response curve, solved from what a world asked for.
//!
//! Split from the spec because these are different jobs: [`super::SimulationSpec`]
//! is what an author *declares*, and this is the arithmetic that turns a declared
//! intent ("the optimum should sit 3x above today's spend") into constants nobody
//! can hold in their head. It is also the only place in the crate that knows what
//! shape a mechanism has.

use crate::SimulationError;

use super::CalibrateSpec;

/// The solved response curve: `incremental_sales(s) = scale * s^theta`.
///
/// Saturating by construction. A linear response would make the optimum infinite
/// — every unit of spend returning `margin × slope` for ever — and a profit race
/// against an unbounded optimum measures nothing but the per-period clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResponseCurve {
    pub theta: f64,
    pub scale: f64,
    /// Opening spend per entity-day, the point the calibration is anchored at.
    pub anchor_spend: f64,
    /// Profit-maximising spend per entity-day, under the declared margin.
    pub optimum_spend: f64,
}

impl ResponseCurve {
    /// Incremental sales produced by `spend` on one entity-day.
    pub fn response(&self, spend: f64) -> f64 {
        if spend <= 0.0 {
            return 0.0;
        }
        self.scale * spend.powf(self.theta)
    }

    /// `d(response)/d(spend)` — what a linear fit at this point would measure.
    pub fn local_slope(&self, spend: f64) -> f64 {
        if spend <= 0.0 {
            return f64::INFINITY;
        }
        self.scale * self.theta * spend.powf(self.theta - 1.0)
    }
}

impl CalibrateSpec {
    /// Solve the curve from the declared intent.
    ///
    /// With `R(s) = a·s^θ` and margin `m`, profit is `m·R(s) − s`, so the optimum
    /// sits where `m·R'(s) = 1`. Since `R'` is proportional to `s^(θ−1)`, the
    /// ratio between the optimum and the opening spend fixes θ:
    ///
    /// ```text
    ///   (s*/s₀)^(θ−1) = 1 / (m · slope₀)      ⇒   θ = 1 + ln(1/(m·slope₀)) / ln(s*/s₀)
    /// ```
    ///
    /// then `a` follows from the slope at `s₀`.
    pub fn solve(
        &self,
        margin: f64,
        baseline_sales: f64,
    ) -> Result<ResponseCurve, SimulationError> {
        let anchor_spend = self.anchor_spend_share * baseline_sales;
        if anchor_spend <= 0.0 {
            return Err(SimulationError::Spec(
                "anchor_spend_share and sales_per_entity_day must both be positive".into(),
            ));
        }

        // Opening marginal profit must be positive, or the optimum lies *below*
        // the opening spend and asking for it above is contradictory.
        let opening_return = margin * self.local_slope_at_anchor;
        if opening_return <= 1.0 {
            return Err(SimulationError::Spec(format!(
                "margin {margin} × local_slope_at_anchor {} = {opening_return:.3}, so a unit of \
                 spend already loses money at the opening level and the optimum is below it, not \
                 above. Raise local_slope_at_anchor above {:.3}.",
                self.local_slope_at_anchor,
                1.0 / margin
            )));
        }

        // θ must land in (0, 1): θ ≥ 1 never saturates, θ ≤ 0 is not a response.
        // Working the algebra through, that bounds the reachable optimum from
        // below — and the floor is not obvious, so name it in the error rather
        // than reporting an out-of-range θ the author has to invert by hand.
        let floor = opening_return;
        if self.optimum_at <= floor {
            return Err(SimulationError::Spec(format!(
                "optimum_at {} is unreachable: with margin {margin} and local_slope_at_anchor {}, \
                 the optimum cannot sit closer than {floor:.3}× the opening spend for any \
                 saturating curve. Ask for a larger optimum_at, or a smaller local_slope_at_anchor.",
                self.optimum_at, self.local_slope_at_anchor
            )));
        }

        let theta = 1.0 + (1.0 / opening_return).ln() / self.optimum_at.ln();
        let scale = self.local_slope_at_anchor * anchor_spend.powf(1.0 - theta) / theta;
        let optimum_spend = self.optimum_at * anchor_spend;

        Ok(ResponseCurve {
            theta,
            scale,
            anchor_spend,
            optimum_spend,
        })
    }
}
