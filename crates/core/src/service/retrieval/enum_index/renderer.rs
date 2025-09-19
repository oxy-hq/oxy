use std::collections::{HashMap, HashSet};

use aho_corasick::AhoCorasick;
use minijinja::{Value, context};

use super::types::{EnumRoutingBlob, Match, TemplateSpec};
use crate::{errors::OxyError, execute::renderer::Renderer};

pub(crate) fn find_enum_matches(ac: &AhoCorasick, query: &str) -> Vec<Match> {
    ac.find_iter(query)
        .map(|m| Match {
            pattern_id: m.pattern().as_u32(),
            start: m.start(),
            end: m.end(),
        })
        .collect()
}

pub(crate) fn get_templates_to_render<'a>(
    routing: &'a EnumRoutingBlob,
    matches: &[Match],
) -> Vec<&'a TemplateSpec> {
    // Build a bitmask of enum variables that were matched in the query
    let mut matched_mask: u64 = 0;
    for m in matches {
        let pid = m.pattern_id as usize;
        if pid >= routing.pattern_to_lex.len() {
            continue;
        }
        for entry in &routing.pattern_to_lex[pid] {
            let var_id = entry.var_id as usize;
            if var_id < 64 {
                matched_mask |= 1u64 << var_id as u64;
            }
        }
    }

    // Collect candidate templates via var_to_templates
    let mut template_ids: HashSet<u32> = HashSet::new();
    for m in matches {
        let pid = m.pattern_id as usize;
        if pid >= routing.pattern_to_lex.len() {
            continue;
        }
        for entry in &routing.pattern_to_lex[pid] {
            let var_id = entry.var_id as usize;
            if let Some(list) = routing.var_to_templates.get(var_id) {
                for &tid in list.iter() {
                    template_ids.insert(tid);
                }
            }
        }
    }

    // Enforce full enum variable coverage: only keep templates where all enum vars used
    // by the template are present in the matched set
    template_ids
        .into_iter()
        .filter_map(|tid| routing.templates.get(tid as usize))
        .filter(|t| t.enum_vars_mask != 0 && (t.enum_vars_mask & matched_mask) == t.enum_vars_mask)
        .collect()
}

pub(crate) fn render_enum_template(
    template: &TemplateSpec,
    matches: &[Match],
    routing: &EnumRoutingBlob,
) -> Result<String, OxyError> {
    let var_to_value = build_var_to_value_map(matches, routing);

    // Build a masked version of the template using precomputed non-enum spans
    // This is so non-enums are not rendered and the jinja syntax is preserved
    let (masked_template, restorations) = mask_non_enum_spans(template);

    // Build nested context for dotted vars
    let mut nested: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for (k, v) in var_to_value.into_iter() {
        set_nested(&mut nested, &k, serde_json::Value::String(v));
    }

    // Use an empty global context to avoid injecting concrete non-enum values.
    let renderer = Renderer::new(context! {});
    renderer.register_template(&masked_template)?;
    let current_ctx = Value::from_serialize(serde_json::Value::Object(nested));
    let mut rendered = renderer.wrap(&current_ctx).render(&masked_template)?;

    // Restore masked non-enum placeholders back to their original Jinja syntax
    rendered = restore_masked_spans(rendered, restorations);
    Ok(rendered)
}

fn build_var_to_value_map(matches: &[Match], routing: &EnumRoutingBlob) -> HashMap<String, String> {
    let mut var_to_value: HashMap<String, String> = HashMap::new();
    for m in matches {
        let pid = m.pattern_id as usize;
        if pid >= routing.patterns.len() || pid >= routing.pattern_to_lex.len() {
            continue;
        }
        let enum_value = routing.patterns[pid].clone();
        for entry in &routing.pattern_to_lex[pid] {
            let idx = entry.var_id as usize;
            if idx < routing.var_names.len() {
                let var_name = routing.var_names[idx].clone();
                var_to_value
                    .entry(var_name)
                    .and_modify(|existing| {
                        if enum_value.len() > existing.len() {
                            *existing = enum_value.clone();
                        }
                    })
                    .or_insert(enum_value.clone());
            }
        }
    }
    var_to_value
}

fn mask_non_enum_spans(template: &TemplateSpec) -> (String, Vec<(String, String)>) {
    let spans_iter = template
        .vars
        .iter()
        .filter(|var| !var.is_enum)
        .map(|var| (var.span.start as usize, var.span.end as usize));
    let mut masked_template = String::with_capacity(template.template.len());
    let mut last_idx = 0usize;
    let mut restorations: Vec<(String, String)> = Vec::new(); // (token, original)
    let mut counter: usize = 0;
    for (start, end) in spans_iter {
        if start > last_idx {
            masked_template.push_str(&template.template[last_idx..start]);
        }
        let original = template.template[start..end].to_string();
        let token = format!("__OXY_MASK_{}__", counter);
        counter += 1;
        masked_template.push_str(&token);
        restorations.push((token, original));
        last_idx = end;
    }
    if last_idx < template.template.len() {
        masked_template.push_str(&template.template[last_idx..]);
    }
    (masked_template, restorations)
}

fn restore_masked_spans(mut rendered: String, restorations: Vec<(String, String)>) -> String {
    for (token, original) in restorations {
        rendered = rendered.replace(&token, &original);
    }
    rendered
}

/// Set a dotted value in a JSON map
///
/// This is a helper function to set a value in a nested JSON map using a dotted path.
fn set_nested(
    map: &mut serde_json::Map<String, serde_json::Value>,
    dotted: &str,
    value: serde_json::Value,
) {
    let mut cursor = map;
    let mut parts = dotted.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            cursor.insert(part.to_string(), value);
            break;
        }
        cursor = cursor
            .entry(part.to_string())
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap();
    }
}
