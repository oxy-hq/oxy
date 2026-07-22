// `pub(crate)` rather than private so in-crate tests can build metric fixtures
// (`types::{Correctness, Similarity, RunStats}`) without widening the public API.
pub(crate) mod builders;
mod reporters;

pub use builders::EvalLauncher;
pub use builders::types::{EvalInput, EvalResult, MetricKind};
pub use reporters::{JsonReporter, PrettyReporter, Reporter};
