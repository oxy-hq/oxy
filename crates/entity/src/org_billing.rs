use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum BillingStatus {
    #[sea_orm(string_value = "incomplete")]
    Incomplete,
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "past_due")]
    PastDue,
    #[sea_orm(string_value = "unpaid")]
    Unpaid,
    #[sea_orm(string_value = "canceled")]
    Canceled,
}

impl BillingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BillingStatus::Incomplete => "incomplete",
            BillingStatus::Active => "active",
            BillingStatus::PastDue => "past_due",
            BillingStatus::Unpaid => "unpaid",
            BillingStatus::Canceled => "canceled",
        }
    }
}

/// Type-only enum for `BillingSummary` API responses. Re-derived from the
/// seat item's Stripe `Price.recurring.interval` per request — never stored
/// locally in `org_billing` after the 2026-04-28 redesign.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BillingCycle {
    Monthly,
    Annual,
}

impl BillingCycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            BillingCycle::Monthly => "monthly",
            BillingCycle::Annual => "annual",
        }
    }

    pub fn from_stripe_interval(interval: &str) -> Option<Self> {
        match interval {
            "month" => Some(Self::Monthly),
            "year" => Some(Self::Annual),
            _ => None,
        }
    }
}

impl FromStr for BillingCycle {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "monthly" => Ok(Self::Monthly),
            "annual" => Ok(Self::Annual),
            _ => Err(format!("Invalid billing cycle: {s}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_billing")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub org_id: Uuid,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub stripe_subscription_seat_item_id: Option<String>,
    pub status: BillingStatus,
    pub current_period_start: Option<DateTimeWithTimeZone>,
    pub current_period_end: Option<DateTimeWithTimeZone>,
    pub grace_period_ends_at: Option<DateTimeWithTimeZone>,
    pub seats_paid: i32,
    pub payment_action_url: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Organizations,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
