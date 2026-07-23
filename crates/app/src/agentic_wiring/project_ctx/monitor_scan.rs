//! [`MonitorScanPort`] implementation for [`OxyProjectContext`].

use agentic_automation::WorkspaceContext;
use agentic_pipeline::platform::{MonitorScanPort, ProjectContext};
use async_trait::async_trait;

use super::OxyProjectContext;

#[async_trait]
impl MonitorScanPort for OxyProjectContext {
    async fn run_monitor_scan(
        &self,
        db: &sea_orm::DatabaseConnection,
        workspace_id: uuid::Uuid,
        granularity: &str,
    ) -> Result<String, String> {
        use oxy_metric_monitoring::Granularity;

        let runner = self
            .metric_tree_runner_system()
            .ok_or_else(|| "no metric tree runner available".to_string())?;
        let gran = match granularity {
            "day" => Granularity::Day,
            "week" => Granularity::Week,
            "month" => Granularity::Month,
            other => return Err(format!("unknown granularity: {other:?}")),
        };
        let config_path = oxy_metric_monitoring::default_config_path(self.workspace_path());
        let scan = oxy_metric_monitoring::scan_workspace(
            runner,
            &config_path,
            chrono::Utc::now(),
            Some(gran),
        )
        .await
        .map_err(|e| e.to_string())?;
        let persisted = oxy_metric_monitoring::upsert_anomalies(db, workspace_id, &scan)
            .await
            .map_err(|e| e.to_string())?;
        let summary = format!(
            "scanned={} failed={} persisted={}",
            scan.outcomes.len(),
            scan.failures.len(),
            persisted
        );
        if scan.failures.is_empty() {
            Ok(summary)
        } else {
            Err(summary)
        }
    }
}
