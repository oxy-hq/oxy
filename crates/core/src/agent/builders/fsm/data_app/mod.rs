pub mod config;
mod state;
mod trigger;

pub use state::{CollectInsights, CollectInsightsDelegator, Insights};
pub use trigger::{BuildDataApp, GenerateInsight};
