use std::collections::VecDeque;

use crate::{
    config::constants::{
        ARTIFACT_SOURCE, CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, MARKDOWN_MAX_FENCES, TASK_SOURCE,
    },
    errors::OxyError,
    execute::types::{
        EventKind, Output,
        event::{Event, EventFormat},
    },
};

use super::OutputWriter;

#[derive(Debug, Default)]
pub struct MarkdownWriter {
    task_queue: VecDeque<String>,
    artifact_queue: VecDeque<String>,
    content: String,
}

#[async_trait::async_trait]
impl OutputWriter<String> for MarkdownWriter {
    async fn write_event(&mut self, event: &Event) -> Result<Option<EventFormat>, OxyError> {
        let result = match event.source.kind.as_str() {
            TASK_SOURCE => match &event.kind {
                EventKind::Started { name, .. } => {
                    self.task_queue.push_back(name.clone());
                    Some(EventFormat {
                        content: format!("\n\n<details>\n<summary>{}</summary>\n\n", name),
                        reference: None,
                        is_error: false,
                        kind: event.source.kind.to_string(),
                    })
                }
                EventKind::Finished { .. } => self.task_queue.pop_back().map(|_| EventFormat {
                    content: "\n</details>\n\n".to_string(),
                    reference: None,
                    is_error: false,
                    kind: event.source.kind.to_string(),
                }),
                _ => None,
            },
            ARTIFACT_SOURCE => match &event.kind {
                EventKind::Started { attributes, .. } => {
                    let mut fences_count = MARKDOWN_MAX_FENCES - self.artifact_queue.len();
                    if fences_count < 3 {
                        fences_count = 3;
                    }
                    let prefix = ":".repeat(fences_count);
                    self.artifact_queue.push_back(prefix.clone());
                    Some(EventFormat {
                        content: format!(
                            "\n\n{}artifact{{{}}}\n",
                            prefix,
                            attributes
                                .iter()
                                .map(|(k, v)| format!("{}={}", k, v))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                        reference: None,
                        kind: event.source.kind.to_string(),
                        is_error: false,
                    })
                }
                EventKind::Finished { .. } => {
                    self.artifact_queue.pop_back().map(|prefix| EventFormat {
                        content: format!("\n{}\n\n", prefix),
                        reference: None,
                        is_error: false,
                        kind: event.source.kind.to_string(),
                    })
                }
                _ => None,
            },
            CONSISTENCY_SOURCE => None,
            CONCURRENCY_SOURCE => None,
            _ => match &event.kind {
                EventKind::Updated { chunk } => match &chunk.delta {
                    Output::Prompt(_) => Some(EventFormat {
                        content: "".to_string(),
                        reference: None,
                        is_error: false,
                        kind: event.source.kind.to_string(),
                    }),
                    Output::Text(text) => Some(EventFormat {
                        content: text.to_string(),
                        reference: None,
                        is_error: false,
                        kind: event.source.kind.to_string(),
                    }),
                    Output::Table(table) => {
                        let table_display = table.to_markdown()?;
                        table.clone().into_reference().map(|reference| EventFormat {
                            content: table_display,
                            reference: Some(reference),
                            is_error: false,
                            kind: event.source.kind.to_string(),
                        })
                    }
                    Output::SQL(sql) => {
                        let sql_display = format!("```\n{}\n```\n", sql);
                        Some(EventFormat {
                            content: sql_display,
                            reference: None,
                            is_error: false,
                            kind: event.source.kind.to_string(),
                        })
                    }
                    _ => None,
                },
                _ => None,
            },
        };

        if let Some(event_format) = &result {
            self.write_str(&event_format.content).await?;
        }

        Ok(result)
    }

    async fn write_str(&mut self, value: &str) -> Result<(), OxyError> {
        self.content.push_str(value);
        Ok(())
    }

    async fn finish(self) -> Result<String, OxyError> {
        Ok(self.content)
    }
}
