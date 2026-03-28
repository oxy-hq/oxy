/// Core domain descriptor.
///
/// Implement this trait to describe all the types that flow through the
/// pipeline.  No logic lives here; it is a pure type-level registry.
pub trait Domain: Sized + Send + Sync + 'static {
    /// The initial, user-facing request.
    type Intent: Send + Sync + Clone + 'static;

    /// A structured description of what needs to be done.
    type Spec: Send + Sync + Clone + 'static;

    /// A concrete plan or action sequence derived from the spec.
    type Solution: Send + 'static;

    /// Raw output captured from executing the solution.
    type Result: Send + 'static;

    /// Interpreted, user-facing answer produced from the result.
    type Answer: Send + Sync + Clone + 'static;

    /// Domain knowledge repository available to the solver (e.g. a tool
    /// registry, schema catalog, or retrieval index).
    type Catalog: Send + 'static;

    /// Domain-specific error type.
    type Error: std::fmt::Display + Send + 'static;
}
