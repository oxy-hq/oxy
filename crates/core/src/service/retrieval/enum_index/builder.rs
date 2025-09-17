use std::collections::{HashMap, HashSet};
use indoc::formatdoc;

use crate::{
    adapters::vector_store::RetrievalObject,
    errors::OxyError,
    theme::StyledText,
};

use super::{
    types::{LexEntry, PlaceholderSpan, EnumRoutingBlob, TemplateSpec, TemplateVar, SemanticEnum},
};

/// Private builder that encapsulates state and invariants
struct IndexBuilder {
    var_name_to_id: HashMap<String, u16>,
    var_values: Vec<Vec<String>>, // per-var values to assign stable value_id
    all_patterns: Vec<String>,
    pid_to_lex_entries: Vec<Vec<LexEntry>>, // pattern_id -> [LexEntry]
    templates: Vec<TemplateSpec>,
}

impl IndexBuilder {
    fn new() -> Self {
        Self {
            var_name_to_id: HashMap::new(),
            var_values: Vec::new(),
            all_patterns: Vec::new(),
            pid_to_lex_entries: Vec::new(),
            templates: Vec::new(),
        }
    }

    /// Seed enum variables into internal structures from a list of (var_name, values)
    /// Ensures var_id assignment, stable per-variable value_ids, and links patterns
    /// to LexEntry entries for retrieval.
    fn seed_enum_variables(&mut self, name_values: &[(String, Vec<String>)]) {
        for (name, values) in name_values.iter() {
            let var_id = match self.var_name_to_id.get(name) {
                Some(id) => *id,
                None => {
                    let new_id = self.var_name_to_id.len() as u16;
                    self.var_name_to_id.insert(name.clone(), new_id);
                    // ensure var_values has an entry for this var
                    self.var_values.push(Vec::new());
                    new_id
                }
            };
            for s in values.iter() {
                // assign value_id based on per-var values list
                let values_list = &mut self.var_values[var_id as usize];
                let value_id_u16 = if let Some(idx) = values_list.iter().position(|v| v == s) {
                    idx as u16
                } else {
                    values_list.push(s.clone());
                    (values_list.len() - 1) as u16
                };

                // add/find pattern id for this surface form
                let pid = if let Some(pid) = self.all_patterns.iter().position(|p| p == s) {
                    pid
                } else {
                    let pid = self.all_patterns.len();
                    self.all_patterns.push(s.clone());
                    self.pid_to_lex_entries.push(Vec::new());
                    pid
                };
                if let Some(entries) = self.pid_to_lex_entries.get_mut(pid) {
                    entries.push(LexEntry { var_id, value_id: value_id_u16 });
                }
            }
        }
    }

    fn build_template_specs(&mut self, retrieval_obj: &RetrievalObject) {
        // Build one flat list of (text, is_exclusion)
        let mut entries: Vec<(String, bool)> = Vec::new();
        
        // Add inclusions (non-exclusion entries)
        for inclusion in &retrieval_obj.inclusions {
            entries.push((inclusion.clone(), false));
        }
        
        // Add exclusions
        for exclusion in &retrieval_obj.exclusions {
            entries.push((exclusion.clone(), true));
        }

        for (template, is_exclusion) in entries.into_iter() {
            // Parse all placeholder spans once
            let mut spans = Vec::new();
            let mut i = 0usize;
            while let Some(start) = template[i..].find("{{") {
                let abs_start = i + start;
                if let Some(end_rel) = template[abs_start + 2..].find("}}") {
                    let abs_end = abs_start + 2 + end_rel;
                    let inner = &template[abs_start + 2..abs_end];
                    let trimmed = inner.trim();
                    let var_name = trimmed.split('|').next().unwrap_or("").trim();
                    if !var_name.is_empty() {
                        spans.push((abs_start, abs_end + 2, var_name.to_string()));
                    } else {
                        spans.push((abs_start, abs_end + 2, String::new()));
                    }
                    i = abs_end + 2;
                } else {
                    break;
                }
            }

            // Compute enum variable mask and collect all variables
            let mut enum_vars_mask: u64 = 0;
            let mut vars: Vec<TemplateVar> = Vec::new();
            let mut seen_var_names: HashSet<String> = HashSet::new();
            
            for (start, end, var_name) in spans {
                if var_name.is_empty() {
                    continue;
                }
                
                // Skip duplicates
                if seen_var_names.contains(&var_name) {
                    continue;
                }
                seen_var_names.insert(var_name.clone());
                
                let is_enum = if let Some(&id) = self.var_name_to_id.get(var_name.as_str()) {
                    if (id as usize) < 64 {
                        enum_vars_mask |= 1u64 << id as u64;
                    }
                    true
                } else {
                    false
                };
                
                vars.push(TemplateVar {
                    name: var_name,
                    span: PlaceholderSpan { start: start as u32, end: end as u32 },
                    is_enum,
                });
            }

            // Skip templates with no enum variables
            if enum_vars_mask == 0 { continue; }

            // Collect non-enum variables for warning
            let non_enum_var_names: Vec<String> = vars.iter()
                .filter(|var| !var.is_enum)
                .map(|var| var.name.clone())
                .collect();

            let formatted_non_enum_var_names = non_enum_var_names.iter().map(|var| format!("  • {}", var)).collect::<String>();
            if !non_enum_var_names.is_empty() {
                println!("{}",
                    formatdoc!(
                        "⚠️  WARNING: Non-enum variables were detected in the retrieval config for
                        {}:
                        {}

                        These variables will not be rendered at retrieval time and will
                        likely reduce recall. Note that the workflow `description` is
                        also used for retrieval, just like `retrieval.include` entries.
                        
                        It is recommended that you either reword the templates to avoid
                        using non-enum variables or replace them with enums. Retrieval
                        works well with enums because the full set of values is known.
                        For regular variables, the best we know are sample or default
                        values, which are often not sufficient for high recall.",
                        retrieval_obj.source_identifier,
                        formatted_non_enum_var_names,
                    ).warning()
                );
            }

            self.templates.push(TemplateSpec {
                template,
                is_exclusion,
                source_identifier: retrieval_obj.source_identifier.clone(),
                source_type: retrieval_obj.source_type.clone(),
                enum_vars_mask,
                vars,
            });
        }
    }

    fn finish(self) -> EnumRoutingBlob {
        // Build var_id -> name mapping (dense 0..N)
        let mut var_names: Vec<String> = vec![String::new(); self.var_name_to_id.len()];
        for (name, id) in self.var_name_to_id.iter() {
            let idx = *id as usize;
            if idx < var_names.len() {
                var_names[idx] = name.clone();
            }
        }

        // Build var_id -> template ids referencing that var using vars
        let mut var_to_templates: Vec<Vec<u32>> = vec![Vec::new(); var_names.len()];
        for (tid, t) in self.templates.iter().enumerate() {
            for var in t.vars.iter() {
                if var.is_enum {
                    if let Some(var_id) = self.var_name_to_id.get(&var.name) {
                        var_to_templates[*var_id as usize].push(tid as u32);
                    }
                }
            }
        }

        EnumRoutingBlob {
            patterns: self.all_patterns,
            pattern_to_lex: self.pid_to_lex_entries,
            templates: self.templates,
            var_names,
            var_to_templates,
        }
    }
}

/// Build routing blob entirely in-memory from configuration
pub(crate) fn build_routing_blob(retrieval_objects: &[RetrievalObject], semantic_enums: &[SemanticEnum]) -> Result<EnumRoutingBlob, OxyError> {
    let mut builder = IndexBuilder::new();

    // 1) Seed with semantic dimensions enums (e.g., dimensions.month)
    builder.seed_enum_variables(semantic_enums);

    // 2) Scan retrieval objects for enum variables and parameterized templates
    for retrieval_obj in retrieval_objects {
        if let Some(enum_vars) = &retrieval_obj.enum_variables {
            let mut obj_enum_pairs: Vec<(String, Vec<String>)> = Vec::new();
            for (name, values) in enum_vars.iter() {
                let strings: Vec<String> = values
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect();
                obj_enum_pairs.push((name.clone(), strings));
            }

            // Seed with enum variables from retrieval object
            builder.seed_enum_variables(&obj_enum_pairs);
        }
        // Pre-build templates necessary for rendering at query time
        builder.build_template_specs(retrieval_obj);
    }

    Ok(builder.finish())
}
