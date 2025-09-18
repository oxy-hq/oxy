use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

pub type SemanticEnum = (String, Vec<String>);

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct PlaceholderSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct TemplateVar {
    pub name: String,
    pub span: PlaceholderSpan,
    pub is_enum: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct LexEntry {
    pub var_id: u16,
    pub value_id: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct TemplateSpec {
    pub template: String,
    pub is_exclusion: bool,
    pub source_identifier: String,
    pub source_type: String,
    pub enum_vars_mask: u64,
    pub vars: Vec<TemplateVar>,
}

/// Readable and rkyv-serializable routing blob. Designed for clarity and easy JSON diffs.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct EnumRoutingBlob {
    /// Patterns used to build the Aho-Corasick automaton (surface forms)
    pub patterns: Vec<String>,

    /// For each pattern_id, the list of (var_id, value_id) entries it maps to
    pub pattern_to_lex: Vec<Vec<LexEntry>>,

    /// Template metadata and content
    pub templates: Vec<TemplateSpec>,

    /// Variable id -> variable name (for pretty/debugging and rendering)
    pub var_names: Vec<String>,

    /// For each var_id, the list of template ids that reference that variable
    pub var_to_templates: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub pattern_id: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct RenderedRetrievalTemplate {
    pub rendered_text: String,
    pub is_exclusion: bool,
    pub source_identifier: String,
    pub source_type: String,
    pub original_template: String,
}
