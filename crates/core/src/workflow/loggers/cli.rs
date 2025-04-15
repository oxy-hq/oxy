use std::io::Write;

use crate::{execute::types::Table, theme::StyledText, utils::print_colored_sql};

use super::types::WorkflowLogger;

#[derive(Debug, Clone, Copy)]
pub struct WorkflowCLILogger;

impl WorkflowLogger for WorkflowCLILogger {
    fn log(&self, text: &str) {
        println!("{}", text);
    }

    fn log_sql_query(&self, query: &str) {
        print_colored_sql(query);
    }

    fn log_table_result(&self, table: Table) {
        match table.to_term() {
            Ok(table) => {
                println!("{}", "\nResult:".primary());
                println!("{}", table);
            }
            Err(e) => {
                println!("{}", format!("Error displaying results: {}", e).error());
            }
        }
    }

    fn log_text_chunk(&mut self, chunk: &str, is_finished: bool) {
        if is_finished {
            println!("{}", chunk);
        } else {
            print!("{}", chunk);
            std::io::stdout().flush().unwrap();
        }
    }
}
