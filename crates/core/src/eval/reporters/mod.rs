use std::io::Write;

use super::builders::types::EvalResult;
use crate::errors::OxyError;

pub trait Reporter {
    fn report(&self, results: &[EvalResult], writer: &mut dyn Write) -> Result<(), OxyError>;
}

mod json;
mod pretty;

pub use json::JsonReporter;
pub use pretty::PrettyReporter;
