//! Execution Analytics Module
//!
//! Provides types and storage for execution analytics data,
//! tracking verified vs generated executions across different tool types.

mod storage;
mod types;

pub use storage::ExecutionAnalyticsStorage;
pub use types::{
    AgentExecutionStats, ExecutionCostResponse, ExecutionDetail, ExecutionListResponse,
    ExecutionSummary, ExecutionTimeBucket, ExecutionType, HistogramBucket,
    LatencyHistogramResponse, LatencyPercentilePoint, LatencyPercentilesResponse, LatencyTriple,
    ModelCost, SourceType, model_cost_usd,
};
