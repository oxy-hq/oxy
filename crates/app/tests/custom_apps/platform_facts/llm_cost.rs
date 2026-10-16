//! `llmCostUsd` over a real Postgres: the runs `agentRuns` counts, each with
//! every run it delegated to, priced the way Admin → Metrics prices them.
//!
//! One org of its own, "Spend", so the main world's counts stay as they are.
//! Every seeded run makes LLM calls on Sonnet ($3/M in, $15/M out, $0.30/M
//! cache reads) unless it says otherwise.

use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use uuid::Uuid;

use super::world::{World, at, org};

const SONNET: &str = "claude-sonnet-4-6";
const M: i64 = 1_000_000;

impl World {
    /// A run in `ws` created at `when`, stamped with `metadata` as its seeder
    /// would; returns its id.
    async fn spend_run(
        &self,
        ws: Uuid,
        source: &str,
        parent: Option<&str>,
        when: DateTime<Utc>,
        metadata: Option<Value>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO agentic_runs (id, workspace_id, question, task_status, source_type, \
                 parent_run_id, metadata, attempt, created_at, updated_at) \
                 VALUES ($1, $2, '', 'done', $3, $4, $5, 0, $6, $6)",
                [
                    id.clone().into(),
                    ws.into(),
                    source.into(),
                    parent.map(str::to_string).into(),
                    metadata.into(),
                    when.into(),
                ],
            ))
            .await
            .expect("seed run");
        id
    }

    /// One LLM call on `run`, as the runtime records it: an `llm_start` with
    /// the prompt tokens, an `llm_end` with the model and the rest.
    async fn llm_call(&self, run: &str, model: &str, [input, output, cache_read]: [i64; 3]) {
        let start = json!({ "prompt_tokens": input });
        let end = json!({
            "model": model,
            "output_tokens": output,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": cache_read,
        });
        for (kind, payload) in [("llm_start", start), ("llm_end", end)] {
            self.db
                .execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "INSERT INTO agentic_run_events (run_id, seq, event_type, payload, attempt, \
                     created_at) \
                     SELECT $1, COALESCE(MAX(seq), -1) + 1, $2, $3, 0, now() \
                     FROM agentic_run_events WHERE run_id = $1",
                    [run.into(), kind.into(), payload.into()],
                ))
                .await
                .expect("seed llm event");
        }
    }

    /// "Spend": W1's customer run tree (root, child, grandchild), an unpriced
    /// run, a daemon run with a delegated child, a staff preview, and one run
    /// in `last7d`. Returns the org.
    async fn spend(&self) -> Uuid {
        let org = World::org(&self.db, "Spend").await;
        let ws = World::workspace(&self.db, org, at(9, 1, 0)).await;
        let root = self
            .spend_run(ws, "analytics", None, at(10, 6, 9), None)
            .await;
        self.llm_call(&root, SONNET, [M, M, 0]).await; // $18
        let child = self
            .spend_run(ws, "analytics", Some(&root), at(10, 6, 9), None)
            .await;
        self.llm_call(&child, SONNET, [M, 0, 0]).await; // $3
        // Created in last7d, but its root is W1's: it is W1's cost.
        let grandchild = self
            .spend_run(ws, "analytics", Some(&child), at(10, 13, 9), None)
            .await;
        self.llm_call(&grandchild, SONNET, [0, M, 0]).await; // $15

        let unpriced = self
            .spend_run(ws, "analytics", None, at(10, 6, 10), None)
            .await;
        self.llm_call(&unpriced, "no-such-model", [M, M, 0]).await;

        let daemon = self
            .spend_run(ws, "preagg_cycle", None, at(10, 6, 11), None)
            .await;
        self.llm_call(&daemon, SONNET, [M, M, 0]).await;
        let daemon_child = self
            .spend_run(ws, "analytics", Some(&daemon), at(10, 6, 11), None)
            .await;
        self.llm_call(&daemon_child, SONNET, [M, M, 0]).await;
        let preview = json!({ "trigger": "preview" });
        let staff_preview = self
            .spend_run(ws, "analytics", None, at(10, 6, 12), Some(preview))
            .await;
        self.llm_call(&staff_preview, SONNET, [M, M, 0]).await;

        let recent = self
            .spend_run(ws, "analytics", None, at(10, 13, 9), None)
            .await;
        self.llm_call(&recent, SONNET, [0, 0, M]).await; // $0.30
        org
    }
}

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() < 1e-9
}

#[tokio::test]
async fn llm_cost_is_agent_runs_cost_with_their_delegated_runs() {
    let w = World::new().await;
    let spend = w.spend().await;
    let facts = w.facts().await;
    let o = org(&facts, spend);
    let w1 = o.weekly[1];
    assert_eq!(w1.agent_runs, 2, "the tree's root and the unpriced run");
    assert!(
        close(w1.llm_cost_usd, 36.0),
        "root $18 + child $3 + grandchild $15; no daemon, no daemon child, no preview: got {}",
        w1.llm_cost_usd
    );
    assert_eq!(w1.llm_unpriced_runs, 1, "the run on a model with no price");
    assert_eq!(o.last7d.agent_runs, 1);
    assert!(
        close(o.last7d.llm_cost_usd, 0.30),
        "the recent run only; the grandchild is its root's: got {}",
        o.last7d.llm_cost_usd
    );
    assert_eq!(o.last7d.llm_unpriced_runs, 0);
    assert!(close(o.weekly[0].llm_cost_usd, 0.0));
}

#[tokio::test]
async fn a_run_that_made_no_llm_call_is_neither_cost_nor_unpriced() {
    let w = World::new().await;
    let facts = w.facts().await;
    // Acme's runs carry no LLM events at all.
    let w1 = org(&facts, w.acme).weekly[1];
    assert_eq!(w1.agent_runs, 2);
    assert!(close(w1.llm_cost_usd, 0.0));
    assert_eq!(w1.llm_unpriced_runs, 0);
}
