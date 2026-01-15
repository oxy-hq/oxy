use std::collections::HashMap;

use crate::{
    execute::writer::Handler,
    service::types::{
        block::{Block, BlockKind, Group, GroupKind},
        content::ContentType,
        event::EventKind,
    },
};
use oxy_shared::errors::OxyError;

pub struct BlockHandler {
    block_stack: Vec<String>,
    blocks: HashMap<String, Block>,
    root: Vec<String>,
}

impl Default for BlockHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockHandler {
    pub fn new() -> Self {
        BlockHandler {
            block_stack: Vec::new(),
            blocks: HashMap::new(),
            root: Vec::new(),
        }
    }

    pub fn collect(self) -> (HashMap<String, Block>, Vec<String>) {
        let block_stack = self.block_stack;
        let blocks = self.blocks;
        let root = self.root;

        let blocks = blocks
            .into_iter()
            .map(|(block_id, mut block)| {
                if block_stack.contains(&block_id) {
                    tracing::warn!(
                        "Block with ID {} is still in stack, mark as cancelled",
                        block.id
                    );
                    block.set_error("Cancelled".to_string());
                }
                (block_id, block)
            })
            .collect::<HashMap<String, Block>>();

        (blocks, root)
    }

    pub fn current_block_mut(&mut self) -> Option<&mut Block> {
        self.block_stack
            .last()
            .and_then(|id| self.blocks.get_mut(id))
    }

    pub fn upsert_block(&mut self, block_id: String, block_kind: BlockKind) {
        let parent_id = self
            .block_stack
            .last()
            .cloned()
            .and_then(|p| if p == block_id { None } else { Some(p) });

        match self.blocks.get_mut(&block_id) {
            Some(block) => {
                if let (
                    BlockKind::Text { content },
                    BlockKind::Text {
                        content: update_content,
                    },
                ) = (&mut block.block_kind, block_kind)
                {
                    content.push_str(update_content.as_str());
                }
            } // Block already exists with the same kind
            None => {
                let block = Block::new(block_id.clone(), block_kind);
                self.blocks.insert(block_id.clone(), block);
                if !self.block_stack.iter().any(|id| id == &block_id) {
                    tracing::debug!("Adding block {} to stack", block_id);
                    self.block_stack.push(block_id.clone());
                }
                if parent_id.is_none() {
                    self.root.push(block_id.clone());
                }
            }
        }

        if let Some(parent) = parent_id.and_then(|id| self.blocks.get_mut(&id)) {
            parent.add_child(block_id);
        }
    }

    pub fn add_group_block(&mut self, group_id: String) {
        let mut is_added = false;
        if let Some(block) = self.current_block_mut() {
            block.add_child(group_id.clone());
            is_added = true;
        } else {
            tracing::warn!("No current block to add group block to");
        }

        if is_added {
            self.blocks.insert(
                group_id.clone(),
                Block::new(
                    group_id.clone(),
                    BlockKind::Group {
                        group_id: group_id.clone(),
                    },
                ),
            );
        }
    }

    pub fn finish_block(&mut self, block_id: String, error: Option<String>) {
        self.block_stack.retain(|id| id != &block_id);

        if let Some(block) = self.blocks.get_mut(&block_id) {
            if let Some(error) = error {
                block.set_error(error);
            }
        } else {
            tracing::warn!("Block with ID {} not found", block_id);
        }
    }
}

#[async_trait::async_trait]
impl Handler for BlockHandler {
    type Event = EventKind;

    async fn handle_event(&mut self, event: Self::Event) -> Result<(), OxyError> {
        match event {
            EventKind::TaskStarted {
                task_id,
                task_name,
                task_metadata,
            } => {
                self.upsert_block(
                    task_id.clone(),
                    BlockKind::Task {
                        task_name,
                        task_metadata,
                    },
                );
            }
            EventKind::TaskMetadata { task_id, metadata } => {
                if let Some(block) = self.blocks.get_mut(&task_id) {
                    if let BlockKind::Task {
                        task_name: _,
                        ref mut task_metadata,
                    } = block.block_kind
                    {
                        *task_metadata = Some(metadata);
                    }
                } else {
                    tracing::warn!(
                        "Task block with ID {} not found for metadata update",
                        task_id
                    );
                }
            }
            EventKind::TaskFinished { task_id, error } => {
                self.finish_block(task_id.clone(), error);
            }
            EventKind::StepStarted { step } => {
                self.upsert_block(step.id.clone(), BlockKind::Step(step));
            }
            EventKind::StepFinished { step_id, error } => {
                self.finish_block(step_id, error);
            }
            EventKind::ContentAdded { content_id, item } => {
                self.upsert_block(
                    content_id.clone(),
                    match item {
                        ContentType::Text { content } => BlockKind::Text { content },
                        ContentType::SQL {
                            sql_query,
                            database,
                            result,
                            is_result_truncated,
                        } => BlockKind::SQL {
                            sql_query,
                            database,
                            result,
                            is_result_truncated,
                        },
                        ContentType::DataApp(data_app) => BlockKind::DataApp(data_app),
                        ContentType::SandboxApp { kind, preview_url } => {
                            BlockKind::SandboxApp { kind, preview_url }
                        }
                        ContentType::Viz(viz) => BlockKind::Viz(viz),
                    },
                );
            }
            EventKind::ContentDone { content_id, item } => {
                self.upsert_block(
                    content_id.clone(),
                    match item {
                        ContentType::Text { content } => BlockKind::Text { content },
                        ContentType::SQL {
                            sql_query,
                            database,
                            result,
                            is_result_truncated,
                        } => BlockKind::SQL {
                            sql_query,
                            database,
                            result,
                            is_result_truncated,
                        },
                        ContentType::DataApp(data_app) => BlockKind::DataApp(data_app),
                        ContentType::SandboxApp { kind, preview_url } => {
                            BlockKind::SandboxApp { kind, preview_url }
                        }
                        ContentType::Viz(viz) => BlockKind::Viz(viz),
                    },
                );
                self.finish_block(content_id, None);
            }
            _ => {}
        }
        Ok(())
    }
}

pub struct GroupBlockHandler {
    group_stack: Vec<String>,
    group_blocks: HashMap<String, BlockHandler>,
    groups: HashMap<String, Group>,
}

impl Default for GroupBlockHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupBlockHandler {
    pub fn new() -> Self {
        GroupBlockHandler {
            group_stack: Vec::new(),
            group_blocks: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    pub fn collect(self) -> Vec<Group> {
        let group_stack = self.group_stack;
        let mut group_blocks = self.group_blocks;
        self.groups
            .into_values()
            .map(|mut group| {
                let (blocks, children) = group_blocks
                    .remove(&group.id())
                    .map(|handler| handler.collect())
                    .unwrap_or_default();
                if group_stack.contains(&group.id()) {
                    tracing::warn!(
                        "Group with ID {} is still in stack, mark as cancelled",
                        group.id()
                    );
                    group.set_error("Cancelled".to_string());
                }
                group.with_blocks(blocks, children)
            })
            .collect()
    }

    pub fn start_group(&mut self, group_kind: GroupKind) {
        let group_id = group_kind.id();
        if let Some(handler) = self
            .group_stack
            .last()
            .and_then(|group_id| self.group_blocks.get_mut(group_id))
        {
            handler.add_group_block(group_id.clone());
        }

        if self.group_stack.contains(&group_id) {
            tracing::warn!("Group with ID {} already exists, skipping start", group_id);
            return;
        }
        self.group_stack.push(group_id.clone());
        self.group_blocks
            .insert(group_id.clone(), BlockHandler::new());
        self.groups.insert(group_id.clone(), Group::new(group_kind));
    }

    pub fn end_group(&mut self, group_id: String, error: Option<String>) {
        self.group_stack.retain(|id| id != &group_id);
        if let Some(group) = self.groups.get_mut(&group_id)
            && let Some(err) = error
        {
            group.set_error(err);
        }
    }

    pub async fn forward_event(&mut self, event: EventKind) -> Result<(), OxyError> {
        if let Some(group_id) = self.group_stack.last()
            && let Some(handler) = self.group_blocks.get_mut(group_id)
        {
            return handler.handle_event(event).await;
        } else {
            tracing::warn!("No handler found");
            Ok(())
        }
    }
}

#[async_trait::async_trait]
impl Handler for GroupBlockHandler {
    type Event = EventKind;

    async fn handle_event(&mut self, event: Self::Event) -> Result<(), OxyError> {
        match &event {
            EventKind::WorkflowStarted {
                workflow_id,
                run_id,
                workflow_config,
            } => {
                self.start_group(GroupKind::Workflow {
                    workflow_id: workflow_id.clone(),
                    run_id: run_id.clone(),
                    workflow_config: workflow_config.clone(),
                });
            }
            EventKind::WorkflowFinished {
                workflow_id,
                run_id,
                error,
            } => {
                // Handle workflow finish
                self.end_group(format!("{workflow_id}::{run_id}"), error.clone());
            }
            EventKind::AgenticStarted {
                agent_id,
                run_id,
                agent_config,
            } => {
                self.start_group(GroupKind::Agentic {
                    agent_id: agent_id.clone(),
                    run_id: run_id.clone(),
                    agent_config: agent_config.clone(),
                });
            }
            EventKind::AgenticFinished {
                agent_id,
                run_id,
                error,
            } => {
                // Handle agentic finish
                self.end_group(format!("{agent_id}::{run_id}"), error.clone());
            }
            EventKind::ArtifactStarted {
                artifact_id,
                artifact_name,
                artifact_metadata,
                is_verified,
            } => {
                self.start_group(GroupKind::Artifact {
                    artifact_id: artifact_id.clone(),
                    artifact_name: artifact_name.clone(),
                    artifact_metadata: artifact_metadata.clone(),
                    is_verified: *is_verified,
                });
            }
            EventKind::ArtifactFinished { artifact_id, error } => {
                // Handle artifact finish
                self.end_group(artifact_id.clone(), error.clone());
            }
            _ => {
                // Forward the event to the current group handler
                self.forward_event(event).await?;
            }
        }
        Ok(())
    }
}
