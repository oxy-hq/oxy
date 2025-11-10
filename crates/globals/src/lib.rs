pub mod errors;
pub mod inheritance;
pub mod parser;
pub mod reference;
pub mod registry;
pub mod template;
pub mod validator;

pub use errors::{GlobalError, GlobalResult};
pub use inheritance::{InheritanceInfo, ObjectInheritanceEngine};
pub use parser::GlobalParser;
pub use reference::GlobalReference;
pub use registry::GlobalRegistry;
pub use template::{TemplateEngine, TemplateResolver};
pub use validator::GlobalValidator;
