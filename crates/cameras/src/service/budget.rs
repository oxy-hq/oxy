//! Workspace VLM budget read/write + month-end projection (IoT
//! Phase 4c).
//!
//! Operators set a per-workspace monthly ceiling and the camera
//! dashboard surfaces a banner once month-to-date spend or the
//! projected month-end spend threatens it. The projection is a
//! straight-line extrapolation:
//!
//! ```text
//! projected_micros = month_to_date_micros * (days_in_month / days_elapsed)
//! ```
//!
//! Linear projection is intentionally simple — VLM dwell triggers
//! correlate with business hours and store traffic, which a more
//! sophisticated forecast would weight, but the operator's
//! actionable threshold ("am I going to blow the budget?") is the
//! same either way. We can revisit when we have a real seasonality
//! signal.

use crate::service::cost;
use crate::service::{ServiceError, ServiceResult};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use entity::workspaces;
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Snapshot for the Cameras dashboard banner.
#[derive(Debug, Clone, Serialize)]
pub struct BudgetStatus {
    /// Operator-configured ceiling. `None` when unset; UI hides
    /// the banner and offers a "set a budget" link instead.
    pub budget_micros: Option<i64>,
    /// VLM spend from the 1st of the current month (UTC) to now.
    pub month_to_date_micros: i64,
    /// Straight-line extrapolation to the end of the month.
    /// Equals `month_to_date_micros` when we've been at it the
    /// whole month; rises sharply on day 1 because the
    /// denominator is small (no smoothing — operators should
    /// trust the banner only after a few days of data).
    pub projected_month_micros: i64,
    /// Day-of-month (1-indexed) when this snapshot was taken.
    pub day_of_month: u32,
    /// Total days in the current month — denominator of the
    /// projection ratio.
    pub days_in_month: u32,
    /// Pre-computed `month_to_date / budget` (0.0–1.0+). `None`
    /// when no budget is set. Saves the UI a division and a
    /// `null` check per render.
    pub percent_consumed: Option<f64>,
    /// Pre-computed `projected / budget` (0.0–1.0+). `None` when
    /// no budget is set. UI uses this to color the banner amber
    /// (≥0.8), red (≥1.0) etc.
    pub percent_projected: Option<f64>,
}

/// Body for `PATCH /{wid}/cameras/budget`. `None` clears the
/// budget (banner hides). Negative values rejected.
#[derive(Debug, Deserialize)]
pub struct BudgetPayload {
    pub monthly_budget_micros: Option<i64>,
}

/// Read the configured budget. Returns `None` when the workspace
/// doesn't exist (route layer handles that ← upstream); a
/// workspace with the column unset returns `Some(workspace)` with
/// `monthly_vlm_budget_micros = None`.
pub async fn get_budget(db: &DatabaseConnection, workspace_id: Uuid) -> ServiceResult<Option<i64>> {
    let row = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok(row.monthly_vlm_budget_micros)
}

/// Persist a new budget value. `None` clears it (banner hides).
/// Negative inputs return `InvalidInput` rather than silently
/// flipping sign — an operator typo shouldn't quietly disable the
/// guard.
pub async fn set_budget(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    budget_micros: Option<i64>,
) -> ServiceResult<Option<i64>> {
    if let Some(v) = budget_micros {
        if v < 0 {
            return Err(ServiceError::InvalidInput(
                "monthly_budget_micros must be >= 0 or null".into(),
            ));
        }
    }
    let existing = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let am = workspaces::ActiveModel {
        id: Set(existing.id),
        monthly_vlm_budget_micros: Set(budget_micros),
        ..Default::default()
    };
    workspaces::Entity::update(am).exec(db).await?;
    Ok(budget_micros)
}

/// Compute the status snapshot. Reads the budget and runs a
/// month-to-date cost query against Airhouse; both are cheap
/// (single Postgres row + bounded DuckLake SELECT).
pub async fn get_status(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<BudgetStatus> {
    let budget_micros = get_budget(db, workspace_id).await?;
    let now = Utc::now();
    let (start_of_month, day_of_month, days_in_month) = month_bounds(now);
    let month_to_date_micros =
        cost::workspace_total_since(db, workspace_id, start_of_month).await?;
    let projected_month_micros = project_to_eom(month_to_date_micros, day_of_month, days_in_month);
    let (percent_consumed, percent_projected) = match budget_micros {
        Some(b) if b > 0 => (
            Some(month_to_date_micros as f64 / b as f64),
            Some(projected_month_micros as f64 / b as f64),
        ),
        // A `Some(0)` budget is operator-set "do not exceed 0";
        // we report it as "100% consumed" the moment any spend
        // shows up rather than dividing by zero.
        Some(0) if month_to_date_micros > 0 => (Some(f64::INFINITY), Some(f64::INFINITY)),
        Some(0) => (Some(0.0), Some(0.0)),
        _ => (None, None),
    };
    Ok(BudgetStatus {
        budget_micros,
        month_to_date_micros,
        projected_month_micros,
        day_of_month,
        days_in_month,
        percent_consumed,
        percent_projected,
    })
}

// ── Internals ───────────────────────────────────────────────────────────────

/// `(start_of_month_utc, current_day_of_month, days_in_month)`.
/// Anchored on UTC — Airhouse `received_at` is also UTC so no
/// timezone adjustment needed. A workspace whose operators live
/// in a non-UTC timezone may see month boundaries shift by ≤24h;
/// fine for a budget projection but worth noting if we ever
/// surface this in a billing context.
fn month_bounds(now: DateTime<Utc>) -> (DateTime<Utc>, u32, u32) {
    let start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .expect("day=1 always valid for any month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight always valid");
    // Total days in this month = day-1 of next month, minus a day.
    let (next_year, next_month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let next_first = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("day=1 always valid for any month");
    let days_in_month = (next_first - start.date()).num_days() as u32;
    let start_utc = Utc.from_utc_datetime(&start);
    (start_utc, now.day(), days_in_month)
}

/// Linear extrapolation. `day_of_month` is 1-indexed so the
/// denominator is never zero. Saturates at `i64::MAX` to keep the
/// JSON serializer well-behaved on a freak spike.
fn project_to_eom(month_to_date: i64, day_of_month: u32, days_in_month: u32) -> i64 {
    if day_of_month == 0 {
        return month_to_date;
    }
    let ratio = days_in_month as f64 / day_of_month as f64;
    let projected = (month_to_date as f64) * ratio;
    if projected.is_finite() && projected <= i64::MAX as f64 {
        projected.round() as i64
    } else {
        i64::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_handles_year_rollover() {
        let dec_15 = Utc.with_ymd_and_hms(2025, 12, 15, 9, 30, 0).unwrap();
        let (start, day, days) = month_bounds(dec_15);
        assert_eq!(start.year(), 2025);
        assert_eq!(start.month(), 12);
        assert_eq!(start.day(), 1);
        assert_eq!(day, 15);
        assert_eq!(days, 31);
    }

    #[test]
    fn month_bounds_february_leap() {
        let feb_5_2024 = Utc.with_ymd_and_hms(2024, 2, 5, 0, 0, 0).unwrap();
        let (_, _, days) = month_bounds(feb_5_2024);
        assert_eq!(days, 29);
        let feb_5_2025 = Utc.with_ymd_and_hms(2025, 2, 5, 0, 0, 0).unwrap();
        let (_, _, days) = month_bounds(feb_5_2025);
        assert_eq!(days, 28);
    }

    #[test]
    fn project_to_eom_linear() {
        // Day 10 of a 30-day month with $1.00 spent so far → $3.00.
        assert_eq!(project_to_eom(1_000_000, 10, 30), 3_000_000);
        // Last day of the month — projection equals MTD.
        assert_eq!(project_to_eom(5_000_000, 30, 30), 5_000_000);
        // Day 0 (shouldn't happen but guard the divide-by-zero).
        assert_eq!(project_to_eom(100, 0, 30), 100);
    }

    #[test]
    fn project_to_eom_saturates() {
        // i64::MAX as MTD on day 1 would overflow; we clamp.
        let s = project_to_eom(i64::MAX, 1, 30);
        assert_eq!(s, i64::MAX);
    }
}
