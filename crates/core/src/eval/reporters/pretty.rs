use std::io::Write;

use crate::{errors::OxyError, eval::builders::types::EvalResult, theme::StyledText};

use super::Reporter;

pub struct PrettyReporter {
    pub quiet: bool,
}

impl Reporter for PrettyReporter {
    fn report(&self, results: &[EvalResult], writer: &mut dyn Write) -> Result<(), OxyError> {
        for result in results {
            // Write errors to stderr if present
            if !result.errors.is_empty() {
                eprintln!(
                    "{}",
                    format!("\nFailed to generate {} outputs:\n", result.errors.len()).warning()
                );
                eprintln!("**********\n");
                for error in &result.errors {
                    eprintln!("{error}");
                    eprintln!("**********\n");
                }
            }

            // Write success message and metrics
            writeln!(writer, "{}", "✅Eval finished with metrics:".primary())?;
            for metric in &result.metrics {
                writeln!(writer, "{}", format!("{metric}").primary())?;
            }

            // Verbose output if not quiet
            if !self.quiet {
                for metric in &result.metrics {
                    metric.verbose_write(writer)?;
                }
            }
        }
        Ok(())
    }
}
