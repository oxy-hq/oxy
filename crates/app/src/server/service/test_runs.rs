use chrono::Utc;
use entity::{test_case_human_verdicts, test_project_runs, test_run_cases, test_runs};
use oxy::database::client::establish_connection;
use oxy_shared::errors::OxyError;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

// --- TestProjectRun types ---

#[derive(Debug, Serialize, Clone)]
pub struct TestProjectRunInfo {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Aggregate score (0.0–1.0) across all files in this project run.
    pub score: Option<f64>,
    /// Per-file score breakdown.
    pub file_scores: Vec<FileScore>,
    /// Total number of test cases across all files.
    pub total_cases: Option<i64>,
    /// Consistency (0.0–1.0): avg(passing_runs / total_runs) per case.
    pub consistency: Option<f64>,
    /// Sum of avg_duration_ms across all cases (milliseconds).
    pub total_duration_ms: Option<f64>,
    /// Sum of input + output tokens across all cases.
    pub total_tokens: Option<i64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct FileScore {
    pub source_id: String,
    pub run_index: i32,
    pub score: Option<f64>,
}

impl TestProjectRunInfo {
    fn from_model(
        m: test_project_runs::Model,
        score: Option<f64>,
        file_scores: Vec<FileScore>,
        total_cases: Option<i64>,
        consistency: Option<f64>,
        total_duration_ms: Option<f64>,
        total_tokens: Option<i64>,
    ) -> Self {
        TestProjectRunInfo {
            id: m.id,
            project_id: m.project_id,
            name: m.name,
            created_at: m.created_at.with_timezone(&Utc),
            score,
            file_scores,
            total_cases,
            consistency,
            total_duration_ms,
            total_tokens,
        }
    }
}

// --- TestRunInfo types ---

#[derive(Debug, Serialize, Clone)]
pub struct TestRunInfo {
    pub id: Uuid,
    pub source_id: String,
    pub run_index: i32,
    pub project_id: Uuid,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub project_run_id: Option<Uuid>,
    /// Aggregate score (0.0–1.0) computed from case results, None if no cases recorded.
    pub score: Option<f64>,
}

impl TestRunInfo {
    fn from_model(m: test_runs::Model, score: Option<f64>) -> Self {
        TestRunInfo {
            id: m.id,
            source_id: m.source_id,
            run_index: m.run_index,
            project_id: m.project_id,
            name: m.name,
            created_at: m.created_at.with_timezone(&Utc),
            project_run_id: m.project_run_id,
            score,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct TestRunCaseResult {
    pub id: Uuid,
    pub case_index: i32,
    pub prompt: String,
    pub expected: String,
    pub actual_output: Option<String>,
    pub score: f64,
    pub verdict: String,
    pub passing_runs: i32,
    pub total_runs: i32,
    pub avg_duration_ms: Option<f64>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub judge_reasoning: Option<serde_json::Value>,
    pub errors: Option<serde_json::Value>,
    pub human_verdict: Option<String>,
}

impl From<test_run_cases::Model> for TestRunCaseResult {
    fn from(m: test_run_cases::Model) -> Self {
        TestRunCaseResult {
            id: m.id,
            case_index: m.case_index,
            prompt: m.prompt,
            expected: m.expected,
            actual_output: m.actual_output,
            score: m.score,
            verdict: m.verdict,
            passing_runs: m.passing_runs,
            total_runs: m.total_runs,
            avg_duration_ms: m.avg_duration_ms,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            judge_reasoning: m.judge_reasoning,
            errors: m.errors,
            human_verdict: None,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct HumanVerdictInfo {
    pub run_index: i32,
    pub case_index: i32,
    pub verdict: String,
}

impl From<test_case_human_verdicts::Model> for HumanVerdictInfo {
    fn from(m: test_case_human_verdicts::Model) -> Self {
        HumanVerdictInfo {
            run_index: m.run_index,
            case_index: m.case_index,
            verdict: m.verdict,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TestRunWithCases {
    #[serde(flatten)]
    pub run: TestRunInfo,
    pub cases: Vec<TestRunCaseResult>,
}

pub struct InsertCaseData {
    pub case_index: i32,
    pub prompt: String,
    pub expected: String,
    pub actual_output: Option<String>,
    pub score: f64,
    pub verdict: String,
    pub passing_runs: i32,
    pub total_runs: i32,
    pub avg_duration_ms: Option<f64>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub judge_reasoning: Option<serde_json::Value>,
    pub errors: Option<serde_json::Value>,
}

struct RunMetrics {
    avg_score: Option<f64>,
    case_count: i64,
    total_duration_ms: Option<f64>,
    total_tokens: Option<i64>,
    total_passing_runs: i64,
    total_total_runs: i64,
}

pub struct TestRunsManager {
    db: DatabaseConnection,
    project_id: Uuid,
}

impl TestRunsManager {
    pub async fn new(project_id: Uuid) -> Result<Self, OxyError> {
        let db = establish_connection().await.map_err(|e| {
            OxyError::DBError(format!("Failed to establish database connection: {e}"))
        })?;
        Ok(TestRunsManager { db, project_id })
    }

    // --- Project Run methods ---

    pub async fn new_project_run(
        &self,
        name: Option<String>,
    ) -> Result<TestProjectRunInfo, OxyError> {
        let now = Utc::now().into();
        let model = test_project_runs::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(self.project_id),
            name: Set(name),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let result = model
            .insert(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to create project run: {e}")))?;
        Ok(TestProjectRunInfo::from_model(
            result,
            None,
            vec![],
            None,
            None,
            None,
            None,
        ))
    }

    pub async fn list_project_runs(&self) -> Result<Vec<TestProjectRunInfo>, OxyError> {
        let project_runs = test_project_runs::Entity::find()
            .filter(test_project_runs::Column::ProjectId.eq(self.project_id))
            .order_by_desc(test_project_runs::Column::CreatedAt)
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to list project runs: {e}")))?;

        if project_runs.is_empty() {
            return Ok(vec![]);
        }

        let project_run_ids: Vec<Uuid> = project_runs.iter().map(|r| r.id).collect();

        // Get all test_runs for these project runs
        let file_runs = test_runs::Entity::find()
            .filter(test_runs::Column::ProjectRunId.is_in(project_run_ids.clone()))
            .filter(test_runs::Column::ProjectId.eq(self.project_id))
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to list file runs: {e}")))?;

        // Aggregate metrics per test_run (score, cases, duration, tokens, consistency)
        let run_ids: Vec<Uuid> = file_runs.iter().map(|r| r.id).collect();
        let mut metrics_map = if run_ids.is_empty() {
            HashMap::new()
        } else {
            self.aggregate_run_metrics(&run_ids).await?
        };

        // Adjust metric scores for human overrides
        if !metrics_map.is_empty() {
            let run_sources: Vec<(Uuid, String, i32)> = file_runs
                .iter()
                .map(|r| (r.id, r.source_id.clone(), r.run_index))
                .collect();
            let mut score_map: HashMap<Uuid, Option<f64>> = metrics_map
                .iter()
                .map(|(id, m)| (*id, m.avg_score))
                .collect();
            self.adjust_scores_for_human_overrides(&run_sources, &mut score_map)
                .await?;
            // Write back adjusted scores
            for (id, adjusted) in score_map {
                if let Some(m) = metrics_map.get_mut(&id) {
                    m.avg_score = adjusted;
                }
            }
        }

        // Group file runs by project_run_id
        let mut file_runs_by_project: HashMap<Uuid, Vec<&test_runs::Model>> = HashMap::new();
        for run in &file_runs {
            if let Some(pr_id) = run.project_run_id {
                file_runs_by_project.entry(pr_id).or_default().push(run);
            }
        }

        let mut result = Vec::with_capacity(project_runs.len());
        for pr in project_runs {
            let file_runs_for_pr = file_runs_by_project
                .get(&pr.id)
                .cloned()
                .unwrap_or_default();

            let file_scores: Vec<FileScore> = file_runs_for_pr
                .iter()
                .map(|r| {
                    let score = metrics_map.get(&r.id).and_then(|m| m.avg_score);
                    FileScore {
                        source_id: r.source_id.clone(),
                        run_index: r.run_index,
                        score,
                    }
                })
                .collect();

            // Aggregate project-level metrics across all file runs
            let (agg_score, total_cases, consistency, total_duration_ms, total_tokens) =
                if file_runs_for_pr.is_empty() {
                    (None, None, None, None, None)
                } else {
                    let mut score_sum = 0.0f64;
                    let mut score_count = 0usize;
                    let mut cases: i64 = 0;
                    let mut passing_sum: i64 = 0;
                    let mut total_sum: i64 = 0;
                    let mut duration_sum = 0.0f64;
                    let mut tokens_sum: i64 = 0;

                    for r in &file_runs_for_pr {
                        if let Some(m) = metrics_map.get(&r.id) {
                            if let Some(s) = m.avg_score {
                                score_sum += s;
                                score_count += 1;
                            }
                            cases += m.case_count;
                            passing_sum += m.total_passing_runs;
                            total_sum += m.total_total_runs;
                            if let Some(d) = m.total_duration_ms {
                                duration_sum += d;
                            }
                            if let Some(t) = m.total_tokens {
                                tokens_sum += t;
                            }
                        }
                    }

                    let agg = if score_count > 0 {
                        Some(score_sum / score_count as f64)
                    } else {
                        None
                    };
                    let consistency = if total_sum > 0 {
                        Some(passing_sum as f64 / total_sum as f64)
                    } else {
                        None
                    };
                    let tc = if cases > 0 { Some(cases) } else { None };
                    let dur = if duration_sum > 0.0 {
                        Some(duration_sum)
                    } else {
                        None
                    };
                    let tok = if tokens_sum > 0 {
                        Some(tokens_sum)
                    } else {
                        None
                    };

                    (agg, tc, consistency, dur, tok)
                };

            result.push(TestProjectRunInfo::from_model(
                pr,
                agg_score,
                file_scores,
                total_cases,
                consistency,
                total_duration_ms,
                total_tokens,
            ));
        }

        Ok(result)
    }

    pub async fn delete_project_run(&self, project_run_id: Uuid) -> Result<(), OxyError> {
        // Delete associated test_runs first (cascades to test_run_cases via FK).
        // Only consider runs that belong to this project (prevents IDOR on child rows).
        let runs = test_runs::Entity::find()
            .filter(test_runs::Column::ProjectRunId.eq(project_run_id))
            .filter(test_runs::Column::ProjectId.eq(self.project_id))
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to find runs for project run: {e}")))?;

        for run in &runs {
            // Human verdicts are keyed by (project_id, source_id, run_index), not by
            // test_run_id, so they are not cascade-deleted when test_runs rows are removed.
            test_case_human_verdicts::Entity::delete_many()
                .filter(test_case_human_verdicts::Column::ProjectId.eq(self.project_id))
                .filter(test_case_human_verdicts::Column::SourceId.eq(run.source_id.clone()))
                .filter(test_case_human_verdicts::Column::RunIndex.eq(run.run_index))
                .exec(&self.db)
                .await
                .map_err(|e| {
                    OxyError::DBError(format!("Failed to delete human verdicts for run: {e}"))
                })?;
            test_runs::Entity::delete_by_id(run.id)
                .exec(&self.db)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to delete file run: {e}")))?;
        }

        // Filter by project_id as well to prevent IDOR: a user from project A cannot
        // delete a project run that belongs to project B by guessing its UUID.
        test_project_runs::Entity::delete_many()
            .filter(test_project_runs::Column::Id.eq(project_run_id))
            .filter(test_project_runs::Column::ProjectId.eq(self.project_id))
            .exec(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to delete project run: {e}")))?;

        Ok(())
    }

    // --- Test Run methods ---

    pub async fn new_run(
        &self,
        source_id: &str,
        name: Option<String>,
        project_run_id: Option<Uuid>,
    ) -> Result<TestRunInfo, OxyError> {
        let run_index = self.next_run_index(source_id).await?;
        let now = Utc::now().into();
        let model = test_runs::ActiveModel {
            id: Set(Uuid::new_v4()),
            source_id: Set(source_id.to_string()),
            run_index: Set(run_index),
            project_id: Set(self.project_id),
            name: Set(name),
            created_at: Set(now),
            updated_at: Set(now),
            project_run_id: Set(project_run_id),
        };
        let result = model
            .insert(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to create test run: {e}")))?;
        Ok(TestRunInfo::from_model(result, None))
    }

    pub async fn list_runs(&self, source_id: &str) -> Result<Vec<TestRunInfo>, OxyError> {
        let runs = test_runs::Entity::find()
            .filter(test_runs::Column::SourceId.eq(source_id))
            .filter(test_runs::Column::ProjectId.eq(self.project_id))
            .order_by_desc(test_runs::Column::RunIndex)
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to list test runs: {e}")))?;

        if runs.is_empty() {
            return Ok(vec![]);
        }

        let run_ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
        let mut score_map = self.aggregate_scores(&run_ids).await?;

        // Adjust scores for human overrides
        let run_sources: Vec<(Uuid, String, i32)> = runs
            .iter()
            .map(|r| (r.id, r.source_id.clone(), r.run_index))
            .collect();
        self.adjust_scores_for_human_overrides(&run_sources, &mut score_map)
            .await?;

        Ok(runs
            .into_iter()
            .map(|r| {
                let score = score_map.get(&r.id).copied().flatten();
                TestRunInfo::from_model(r, score)
            })
            .collect())
    }

    pub async fn get_run(
        &self,
        source_id: &str,
        run_index: i32,
    ) -> Result<Option<TestRunWithCases>, OxyError> {
        let run = test_runs::Entity::find()
            .filter(test_runs::Column::SourceId.eq(source_id))
            .filter(test_runs::Column::RunIndex.eq(run_index))
            .filter(test_runs::Column::ProjectId.eq(self.project_id))
            .one(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to get test run: {e}")))?;

        let Some(run) = run else {
            return Ok(None);
        };

        let cases = test_run_cases::Entity::find()
            .filter(test_run_cases::Column::TestRunId.eq(run.id))
            .order_by_asc(test_run_cases::Column::CaseIndex)
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to get test run cases: {e}")))?;

        let verdicts = self.list_human_verdicts(source_id, run_index).await?;
        let verdict_map: HashMap<i32, String> = verdicts
            .into_iter()
            .map(|v| (v.case_index, v.verdict))
            .collect();
        let cases: Vec<TestRunCaseResult> = cases
            .into_iter()
            .map(|m| {
                let mut c = TestRunCaseResult::from(m);
                c.human_verdict = verdict_map.get(&c.case_index).cloned();
                c
            })
            .collect();

        let score = if cases.is_empty() {
            None
        } else {
            let n = cases.len() as f64;
            let sum: f64 = cases
                .iter()
                .map(|c| Self::human_effective_score(c.score, c.human_verdict.as_deref()))
                .sum();
            Some(sum / n)
        };

        Ok(Some(TestRunWithCases {
            run: TestRunInfo::from_model(run, score),
            cases,
        }))
    }

    pub async fn delete_run(&self, source_id: &str, run_index: i32) -> Result<(), OxyError> {
        let run = test_runs::Entity::find()
            .filter(test_runs::Column::SourceId.eq(source_id))
            .filter(test_runs::Column::RunIndex.eq(run_index))
            .filter(test_runs::Column::ProjectId.eq(self.project_id))
            .one(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to find test run: {e}")))?;

        if let Some(run) = run {
            // Human verdicts are keyed by (project_id, source_id, run_index) and are not
            // cascade-deleted when the test_run row is removed via its FK on test_run_cases.
            test_case_human_verdicts::Entity::delete_many()
                .filter(test_case_human_verdicts::Column::ProjectId.eq(self.project_id))
                .filter(test_case_human_verdicts::Column::SourceId.eq(source_id))
                .filter(test_case_human_verdicts::Column::RunIndex.eq(run_index))
                .exec(&self.db)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to delete human verdicts: {e}")))?;
            test_runs::Entity::delete_by_id(run.id)
                .exec(&self.db)
                .await
                .map_err(|e| OxyError::DBError(format!("Failed to delete test run: {e}")))?;
        }
        Ok(())
    }

    pub async fn insert_case(
        &self,
        test_run_id: Uuid,
        data: InsertCaseData,
    ) -> Result<(), OxyError> {
        let now: chrono::DateTime<chrono::FixedOffset> = Utc::now().into();
        let model = test_run_cases::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            test_run_id: Set(test_run_id),
            case_index: Set(data.case_index),
            prompt: Set(data.prompt),
            expected: Set(data.expected),
            actual_output: Set(data.actual_output),
            score: Set(data.score),
            verdict: Set(data.verdict),
            passing_runs: Set(data.passing_runs),
            total_runs: Set(data.total_runs),
            avg_duration_ms: Set(data.avg_duration_ms),
            input_tokens: Set(data.input_tokens),
            output_tokens: Set(data.output_tokens),
            judge_reasoning: Set(data.judge_reasoning),
            errors: Set(data.errors),
            created_at: Set(now),
        };
        // Use an upsert so concurrent SSE streams for the same case cannot both attempt
        // an INSERT and violate idx_test_run_cases_unique (test_run_id, case_index).
        test_run_cases::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    test_run_cases::Column::TestRunId,
                    test_run_cases::Column::CaseIndex,
                ])
                .update_columns([
                    test_run_cases::Column::Prompt,
                    test_run_cases::Column::Expected,
                    test_run_cases::Column::ActualOutput,
                    test_run_cases::Column::Score,
                    test_run_cases::Column::Verdict,
                    test_run_cases::Column::PassingRuns,
                    test_run_cases::Column::TotalRuns,
                    test_run_cases::Column::AvgDurationMs,
                    test_run_cases::Column::InputTokens,
                    test_run_cases::Column::OutputTokens,
                    test_run_cases::Column::JudgeReasoning,
                    test_run_cases::Column::Errors,
                ])
                .to_owned(),
            )
            .exec(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to upsert test run case: {e}")))?;

        Ok(())
    }

    pub async fn aggregate_scores(
        &self,
        run_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Option<f64>>, OxyError> {
        if run_ids.is_empty() {
            return Ok(HashMap::new());
        }

        #[derive(FromQueryResult)]
        struct RunScore {
            test_run_id: Uuid,
            case_count: i64,
            score_sum: Option<f64>,
        }

        let rows: Vec<RunScore> = test_run_cases::Entity::find()
            .filter(test_run_cases::Column::TestRunId.is_in(run_ids.to_vec()))
            .select_only()
            .column(test_run_cases::Column::TestRunId)
            .column_as(
                Expr::col(test_run_cases::Column::TestRunId).count(),
                "case_count",
            )
            .column_as(Expr::col(test_run_cases::Column::Score).sum(), "score_sum")
            .group_by(test_run_cases::Column::TestRunId)
            .into_model::<RunScore>()
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to aggregate run scores: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let avg = r.score_sum.map(|s| {
                    if r.case_count > 0 {
                        s / r.case_count as f64
                    } else {
                        0.0
                    }
                });
                (r.test_run_id, avg)
            })
            .collect())
    }

    async fn aggregate_run_metrics(
        &self,
        run_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, RunMetrics>, OxyError> {
        if run_ids.is_empty() {
            return Ok(HashMap::new());
        }

        #[derive(FromQueryResult)]
        struct RunAgg {
            test_run_id: Uuid,
            case_count: i64,
            score_sum: Option<f64>,
            total_duration_ms: Option<f64>,
            total_input_tokens: Option<i64>,
            total_output_tokens: Option<i64>,
            total_passing_runs: Option<i64>,
            total_total_runs: Option<i64>,
        }

        let rows: Vec<RunAgg> = test_run_cases::Entity::find()
            .filter(test_run_cases::Column::TestRunId.is_in(run_ids.to_vec()))
            .select_only()
            .column(test_run_cases::Column::TestRunId)
            .column_as(
                Expr::col(test_run_cases::Column::TestRunId).count(),
                "case_count",
            )
            .column_as(Expr::col(test_run_cases::Column::Score).sum(), "score_sum")
            .column_as(
                Expr::col(test_run_cases::Column::AvgDurationMs).sum(),
                "total_duration_ms",
            )
            .column_as(
                Expr::col(test_run_cases::Column::InputTokens).sum(),
                "total_input_tokens",
            )
            .column_as(
                Expr::col(test_run_cases::Column::OutputTokens).sum(),
                "total_output_tokens",
            )
            .column_as(
                Expr::col(test_run_cases::Column::PassingRuns).sum(),
                "total_passing_runs",
            )
            .column_as(
                Expr::col(test_run_cases::Column::TotalRuns).sum(),
                "total_total_runs",
            )
            .group_by(test_run_cases::Column::TestRunId)
            .into_model::<RunAgg>()
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to aggregate run metrics: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let avg_score = r.score_sum.map(|s| {
                    if r.case_count > 0 {
                        s / r.case_count as f64
                    } else {
                        0.0
                    }
                });
                let total_tokens = match (r.total_input_tokens, r.total_output_tokens) {
                    (Some(i), Some(o)) => Some(i + o),
                    (Some(i), None) => Some(i),
                    (None, Some(o)) => Some(o),
                    _ => None,
                };
                (
                    r.test_run_id,
                    RunMetrics {
                        avg_score,
                        case_count: r.case_count,
                        total_duration_ms: r.total_duration_ms,
                        total_tokens,
                        total_passing_runs: r.total_passing_runs.unwrap_or(0),
                        total_total_runs: r.total_total_runs.unwrap_or(0),
                    },
                )
            })
            .collect())
    }

    /// Get all human verdicts for a given source (test file) and run index.
    pub async fn list_human_verdicts(
        &self,
        source_id: &str,
        run_index: i32,
    ) -> Result<Vec<HumanVerdictInfo>, OxyError> {
        let rows = test_case_human_verdicts::Entity::find()
            .filter(test_case_human_verdicts::Column::ProjectId.eq(self.project_id))
            .filter(test_case_human_verdicts::Column::SourceId.eq(source_id))
            .filter(test_case_human_verdicts::Column::RunIndex.eq(run_index))
            .all(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to list human verdicts: {e}")))?;
        Ok(rows.into_iter().map(HumanVerdictInfo::from).collect())
    }

    /// Set or clear a human verdict for a specific case in a specific run.
    pub async fn set_human_verdict(
        &self,
        source_id: &str,
        run_index: i32,
        case_index: i32,
        verdict: Option<String>,
    ) -> Result<Option<HumanVerdictInfo>, OxyError> {
        let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();

        if let Some(verdict) = verdict {
            let existing = test_case_human_verdicts::Entity::find()
                .filter(test_case_human_verdicts::Column::ProjectId.eq(self.project_id))
                .filter(test_case_human_verdicts::Column::SourceId.eq(source_id))
                .filter(test_case_human_verdicts::Column::RunIndex.eq(run_index))
                .filter(test_case_human_verdicts::Column::CaseIndex.eq(case_index))
                .one(&self.db)
                .await
                .map_err(|e| {
                    OxyError::DBError(format!("Failed to check existing human verdict: {e}"))
                })?;

            if let Some(existing) = existing {
                let mut active: test_case_human_verdicts::ActiveModel = existing.into();
                active.verdict = Set(verdict);
                active.updated_at = Set(now);
                let updated = active.update(&self.db).await.map_err(|e| {
                    OxyError::DBError(format!("Failed to update human verdict: {e}"))
                })?;
                Ok(Some(HumanVerdictInfo::from(updated)))
            } else {
                let model = test_case_human_verdicts::ActiveModel {
                    id: ActiveValue::Set(Uuid::new_v4()),
                    project_id: Set(self.project_id),
                    source_id: Set(source_id.to_string()),
                    run_index: Set(run_index),
                    case_index: Set(case_index),
                    verdict: Set(verdict),
                    created_at: Set(now),
                    updated_at: Set(now),
                };
                let inserted = model.insert(&self.db).await.map_err(|e| {
                    OxyError::DBError(format!("Failed to insert human verdict: {e}"))
                })?;
                Ok(Some(HumanVerdictInfo::from(inserted)))
            }
        } else {
            // Clear the verdict
            let existing = test_case_human_verdicts::Entity::find()
                .filter(test_case_human_verdicts::Column::ProjectId.eq(self.project_id))
                .filter(test_case_human_verdicts::Column::SourceId.eq(source_id))
                .filter(test_case_human_verdicts::Column::RunIndex.eq(run_index))
                .filter(test_case_human_verdicts::Column::CaseIndex.eq(case_index))
                .one(&self.db)
                .await
                .map_err(|e| {
                    OxyError::DBError(format!("Failed to find human verdict to delete: {e}"))
                })?;

            if let Some(existing) = existing {
                test_case_human_verdicts::Entity::delete_by_id(existing.id)
                    .exec(&self.db)
                    .await
                    .map_err(|e| {
                        OxyError::DBError(format!("Failed to delete human verdict: {e}"))
                    })?;
            }
            Ok(None)
        }
    }

    /// Apply human verdict score: "pass"→1.0, "fail"→0.0, else keep original.
    fn human_effective_score(original: f64, verdict: Option<&str>) -> f64 {
        match verdict {
            Some("pass") => 1.0,
            Some("fail") => 0.0,
            _ => original,
        }
    }

    /// Load human verdicts for the given runs and recompute per-run avg scores.
    /// Returns an adjusted score map (only entries that changed are overwritten).
    async fn adjust_scores_for_human_overrides(
        &self,
        runs: &[(Uuid, String, i32)], // (run_id, source_id, run_index)
        base_scores: &mut HashMap<Uuid, Option<f64>>,
    ) -> Result<(), OxyError> {
        // Collect unique (source_id, run_index) pairs to keep the DB query tight.
        let source_ids: Vec<String> = runs
            .iter()
            .map(|(_, s, _)| s.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let run_indices: Vec<i32> = runs
            .iter()
            .map(|(_, _, ri)| *ri)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Load human verdicts scoped to the exact (source_id, run_index) pairs being
        // processed rather than all history for those sources.
        let all_hvs = test_case_human_verdicts::Entity::find()
            .filter(test_case_human_verdicts::Column::ProjectId.eq(self.project_id))
            .filter(test_case_human_verdicts::Column::SourceId.is_in(source_ids))
            .filter(test_case_human_verdicts::Column::RunIndex.is_in(run_indices))
            .all(&self.db)
            .await
            .map_err(|e| {
                OxyError::DBError(format!("Failed to load human verdicts for adjustment: {e}"))
            })?;

        if all_hvs.is_empty() {
            return Ok(());
        }

        // Build (source_id, run_index) -> { case_index -> verdict }
        let mut verdict_map: HashMap<(String, i32), HashMap<i32, String>> = HashMap::new();
        for hv in all_hvs {
            verdict_map
                .entry((hv.source_id, hv.run_index))
                .or_default()
                .insert(hv.case_index, hv.verdict);
        }

        // Find runs that have overrides
        let affected_run_ids: Vec<Uuid> = runs
            .iter()
            .filter(|(_, s, ri)| verdict_map.contains_key(&(s.clone(), *ri)))
            .map(|(id, _, _)| *id)
            .collect();

        if affected_run_ids.is_empty() {
            return Ok(());
        }

        // Fetch case-level scores for affected runs
        let cases = test_run_cases::Entity::find()
            .filter(test_run_cases::Column::TestRunId.is_in(affected_run_ids))
            .all(&self.db)
            .await
            .map_err(|e| {
                OxyError::DBError(format!("Failed to fetch cases for score adjustment: {e}"))
            })?;

        // run_id -> (source_id, run_index) lookup
        let run_info: HashMap<Uuid, (&str, i32)> = runs
            .iter()
            .map(|(id, s, ri)| (*id, (s.as_str(), *ri)))
            .collect();

        // Group cases by run
        let mut cases_by_run: HashMap<Uuid, Vec<&test_run_cases::Model>> = HashMap::new();
        for case in &cases {
            cases_by_run.entry(case.test_run_id).or_default().push(case);
        }

        // Recompute avg score for affected runs
        for (run_id, run_cases) in &cases_by_run {
            if let Some(&(source_id, run_index)) = run_info.get(run_id)
                && let Some(verdicts) = verdict_map.get(&(source_id.to_string(), run_index))
            {
                let n = run_cases.len() as f64;
                if n > 0.0 {
                    let sum: f64 = run_cases
                        .iter()
                        .map(|c| {
                            Self::human_effective_score(
                                c.score,
                                verdicts.get(&c.case_index).map(|s| s.as_str()),
                            )
                        })
                        .sum();
                    base_scores.insert(*run_id, Some(sum / n));
                }
            }
        }

        Ok(())
    }

    /// Atomically increment and return the next run index for the given source.
    /// Uses INSERT ... ON CONFLICT DO UPDATE ... RETURNING for atomicity without
    /// advisory locks, matching the pattern used by workflow runs.
    async fn next_run_index(&self, source_id: &str) -> Result<i32, OxyError> {
        let row =
            entity::test_run_sequences::Entity::insert(entity::test_run_sequences::ActiveModel {
                project_id: ActiveValue::Set(self.project_id),
                source_id: ActiveValue::Set(source_id.to_string()),
                last_value: ActiveValue::Set(1),
            })
            .on_conflict(
                OnConflict::columns([
                    entity::test_run_sequences::Column::ProjectId,
                    entity::test_run_sequences::Column::SourceId,
                ])
                .value(
                    entity::test_run_sequences::Column::LastValue,
                    Expr::col((
                        entity::test_run_sequences::Entity,
                        entity::test_run_sequences::Column::LastValue,
                    ))
                    .add(1),
                )
                .to_owned(),
            )
            .exec_with_returning(&self.db)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to advance test run sequence: {e}")))?;
        Ok(row.last_value)
    }
}
