//! Pre-resolution of nested workflow references.
//!
//! [`resolve_subworkflows`] walks a [`WorkflowConfig`]'s task tree,
//! loads each referenced child workflow YAML via
//! [`WorkspaceContext::resolve_workflow_yaml`], and caches the child's
//! tasks in [`SubWorkflowConfig::resolved_tasks`]. The decider then
//! reads `resolved_tasks` to populate the recursive `inner_tasks`
//! payload on `subrun_started`, so the frontend can render a single
//! nested DAG instead of dumping child JSON.
//!
//! Resolution is idempotent and runs once before the workflow starts —
//! the resolved tree is persisted as part of `WorkflowRunState.workflow`
//! so resumes don't re-resolve. Cycle detection prevents infinite
//! recursion when workflow `src:` paths form a loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::pin::Pin;

use agentic_core::subrun::SubrunStep;

use crate::config::{TaskConfig, TaskType, WorkflowConfig};
use crate::workspace::WorkspaceContext;

/// Build a [`SubrunStep`] tree from a parent workflow's task list.
///
/// Container tasks (`loop_sequential` and `workflow`) carry their
/// child tasks in [`SubrunStep::inner_tasks`], so the resulting tree
/// mirrors the full nested DAG that will execute. Used by the
/// orchestrator and decider when emitting `subrun_started` so the
/// frontend can render every level of the tree from a single event.
pub fn build_subrun_steps(tasks: &[TaskConfig]) -> Vec<SubrunStep> {
    tasks.iter().map(build_subrun_step).collect()
}

fn build_subrun_step(task: &TaskConfig) -> SubrunStep {
    let task_type = task.task_type.name().to_string();
    let inner_tasks = match &task.task_type {
        TaskType::LoopSequential(loop_cfg) => build_subrun_steps(&loop_cfg.tasks),
        TaskType::SubWorkflow(sub) => build_subrun_steps(&sub.resolved_tasks),
        // Conditional carries multiple branches — flatten in display order
        // (matching condition list, then else). The decider currently
        // doesn't surface which branch ran in the inner step list, so
        // this is best-effort: the FE renders all candidate children
        // and the inactive ones simply never get results.
        TaskType::Conditional(cond) => {
            let mut out = Vec::new();
            for branch in &cond.conditions {
                out.extend(build_subrun_steps(&branch.tasks));
            }
            if let Some(else_tasks) = &cond.else_tasks {
                out.extend(build_subrun_steps(else_tasks));
            }
            out
        }
        _ => Vec::new(),
    };
    SubrunStep {
        name: task.name.clone(),
        task_type,
        inner_tasks,
    }
}

/// Recursively populate `resolved_tasks` on every `SubWorkflow` task in
/// `tasks`, walking through container task types (`LoopSequential`,
/// `Conditional`) so nested sub-workflows are resolved too.
///
/// `visited` tracks the in-flight resolution stack — a `src` path that
/// already appears on the stack means a cycle, and the sub-workflow's
/// `resolved_tasks` is left empty. The frontend then renders that step
/// generically; the worker still attempts execution at run time and
/// surfaces a normal load error if the cycle actually fires.
///
/// Returns silently on any I/O or parse error — best-effort enrichment,
/// not a hard precondition. Execution doesn't depend on this data.
pub async fn resolve_subworkflows(config: &mut WorkflowConfig, workspace: &dyn WorkspaceContext) {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    resolve_tasks(&mut config.tasks, workspace, &mut visited).await;
}

fn resolve_tasks<'a>(
    tasks: &'a mut [TaskConfig],
    workspace: &'a dyn WorkspaceContext,
    visited: &'a mut HashSet<PathBuf>,
) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        for task in tasks.iter_mut() {
            match &mut task.task_type {
                TaskType::SubWorkflow(sub) => {
                    if !visited.insert(sub.src.clone()) {
                        continue;
                    }
                    let src = sub.src.to_string_lossy().to_string();
                    if let Ok(yaml) = workspace.resolve_workflow_yaml(&src).await
                        && let Ok(mut child) = serde_yaml::from_str::<WorkflowConfig>(&yaml)
                    {
                        resolve_tasks(&mut child.tasks, workspace, visited).await;
                        sub.resolved_tasks = child.tasks;
                    }
                    visited.remove(&sub.src);
                }
                TaskType::LoopSequential(loop_cfg) => {
                    resolve_tasks(&mut loop_cfg.tasks, workspace, visited).await;
                }
                TaskType::Conditional(cond) => {
                    for branch in &mut cond.conditions {
                        resolve_tasks(&mut branch.tasks, workspace, visited).await;
                    }
                    if let Some(else_tasks) = &mut cond.else_tasks {
                        resolve_tasks(else_tasks, workspace, visited).await;
                    }
                }
                _ => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ConditionBranch, ConditionalConfig, FormatterConfig, LoopConfig, SubWorkflowConfig,
    };
    use crate::workspace::WorkspaceContext;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeWorkspace {
        yamls: Mutex<HashMap<String, String>>,
    }

    impl FakeWorkspace {
        fn new(yamls: &[(&str, &str)]) -> Self {
            Self {
                yamls: Mutex::new(
                    yamls
                        .iter()
                        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                        .collect(),
                ),
            }
        }
    }

    #[async_trait]
    impl WorkspaceContext for FakeWorkspace {
        fn workspace_path(&self) -> &std::path::Path {
            std::path::Path::new("/fake")
        }
        fn database_configs(&self) -> Vec<airlayer::DatabaseConfig> {
            vec![]
        }
        async fn list_workflow_files(&self) -> Result<Vec<PathBuf>, String> {
            Ok(vec![])
        }
        async fn resolve_workflow_yaml(&self, workflow_ref: &str) -> Result<String, String> {
            self.yamls
                .lock()
                .unwrap()
                .get(workflow_ref)
                .cloned()
                .ok_or_else(|| format!("no such workflow: {workflow_ref}"))
        }
        async fn get_connector(
            &self,
            _name: &str,
        ) -> Result<std::sync::Arc<dyn agentic_connector::DatabaseConnector>, String> {
            Err("unused".to_string())
        }
        async fn get_integration(
            &self,
            _name: &str,
        ) -> Result<crate::workspace::IntegrationConfig, String> {
            Err("unused".to_string())
        }
    }

    fn parent_with_subworkflow(src: &str) -> WorkflowConfig {
        WorkflowConfig {
            name: "parent".into(),
            description: String::new(),
            variables: None,
            consistency_prompt: None,
            consistency_model: None,
            tasks: vec![TaskConfig {
                name: "call_child".into(),
                task_type: TaskType::SubWorkflow(SubWorkflowConfig {
                    src: src.into(),
                    variables: None,
                    resolved_tasks: vec![],
                }),
                export: None,
                cache: None,
            }],
        }
    }

    #[tokio::test]
    async fn resolves_direct_child() {
        let ws = FakeWorkspace::new(&[(
            "child.workflow.yml",
            "name: child\ntasks:\n  - name: step1\n    type: execute_sql\n    sql: SELECT 1\n",
        )]);
        let mut cfg = parent_with_subworkflow("child.workflow.yml");
        resolve_subworkflows(&mut cfg, &ws).await;
        let TaskType::SubWorkflow(sub) = &cfg.tasks[0].task_type else {
            panic!("expected SubWorkflow task");
        };
        assert_eq!(sub.resolved_tasks.len(), 1);
        assert_eq!(sub.resolved_tasks[0].name, "step1");
    }

    #[tokio::test]
    async fn resolves_recursively_through_loop() {
        let ws = FakeWorkspace::new(&[(
            "child.workflow.yml",
            "name: child\ntasks:\n  - name: step1\n    type: formatter\n    template: hi\n",
        )]);
        let loop_task = TaskConfig {
            name: "outer_loop".into(),
            task_type: TaskType::LoopSequential(LoopConfig {
                values: json!([1, 2]),
                concurrency: 1,
                tasks: vec![TaskConfig {
                    name: "call_child".into(),
                    task_type: TaskType::SubWorkflow(SubWorkflowConfig {
                        src: "child.workflow.yml".into(),
                        variables: None,
                        resolved_tasks: vec![],
                    }),
                    export: None,
                    cache: None,
                }],
            }),
            export: None,
            cache: None,
        };
        let mut cfg = WorkflowConfig {
            name: "parent".into(),
            description: String::new(),
            variables: None,
            consistency_prompt: None,
            consistency_model: None,
            tasks: vec![loop_task],
        };
        resolve_subworkflows(&mut cfg, &ws).await;
        let TaskType::LoopSequential(loop_cfg) = &cfg.tasks[0].task_type else {
            panic!("expected loop task");
        };
        let TaskType::SubWorkflow(sub) = &loop_cfg.tasks[0].task_type else {
            panic!("expected SubWorkflow task inside loop");
        };
        assert_eq!(sub.resolved_tasks.len(), 1);
        assert_eq!(sub.resolved_tasks[0].name, "step1");
    }

    #[tokio::test]
    async fn handles_cycle() {
        let ws = FakeWorkspace::new(&[
            (
                "a.workflow.yml",
                "name: a\ntasks:\n  - name: call_b\n    type: workflow\n    src: b.workflow.yml\n",
            ),
            (
                "b.workflow.yml",
                "name: b\ntasks:\n  - name: call_a\n    type: workflow\n    src: a.workflow.yml\n",
            ),
        ]);
        let mut cfg = parent_with_subworkflow("a.workflow.yml");
        resolve_subworkflows(&mut cfg, &ws).await;
        // The outer A resolved; B inside A resolved; A inside B left
        // empty (cycle detected).
        let TaskType::SubWorkflow(a) = &cfg.tasks[0].task_type else {
            panic!();
        };
        assert_eq!(a.resolved_tasks.len(), 1);
        let TaskType::SubWorkflow(b) = &a.resolved_tasks[0].task_type else {
            panic!();
        };
        assert_eq!(b.resolved_tasks.len(), 1);
        let TaskType::SubWorkflow(a_inner) = &b.resolved_tasks[0].task_type else {
            panic!();
        };
        assert!(
            a_inner.resolved_tasks.is_empty(),
            "cycle should not recurse"
        );
    }

    #[tokio::test]
    async fn missing_child_is_tolerated() {
        let ws = FakeWorkspace::new(&[]);
        let mut cfg = parent_with_subworkflow("missing.workflow.yml");
        resolve_subworkflows(&mut cfg, &ws).await;
        let TaskType::SubWorkflow(sub) = &cfg.tasks[0].task_type else {
            panic!();
        };
        assert!(sub.resolved_tasks.is_empty());
    }

    #[tokio::test]
    async fn conditional_branches_are_walked() {
        let ws = FakeWorkspace::new(&[(
            "child.workflow.yml",
            "name: child\ntasks:\n  - name: step1\n    type: formatter\n    template: hi\n",
        )]);
        let mut cfg = WorkflowConfig {
            name: "parent".into(),
            description: String::new(),
            variables: None,
            consistency_prompt: None,
            consistency_model: None,
            tasks: vec![TaskConfig {
                name: "cond".into(),
                task_type: TaskType::Conditional(ConditionalConfig {
                    conditions: vec![ConditionBranch {
                        condition: "true".into(),
                        tasks: vec![TaskConfig {
                            name: "call_child".into(),
                            task_type: TaskType::SubWorkflow(SubWorkflowConfig {
                                src: "child.workflow.yml".into(),
                                variables: None,
                                resolved_tasks: vec![],
                            }),
                            export: None,
                            cache: None,
                        }],
                    }],
                    else_tasks: Some(vec![TaskConfig {
                        name: "fallback".into(),
                        task_type: TaskType::Formatter(FormatterConfig {
                            template: "x".into(),
                        }),
                        export: None,
                        cache: None,
                    }]),
                }),
                export: None,
                cache: None,
            }],
        };
        resolve_subworkflows(&mut cfg, &ws).await;
        let TaskType::Conditional(cond) = &cfg.tasks[0].task_type else {
            panic!();
        };
        let TaskType::SubWorkflow(sub) = &cond.conditions[0].tasks[0].task_type else {
            panic!();
        };
        assert_eq!(sub.resolved_tasks.len(), 1);
    }
}
