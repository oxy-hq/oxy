use std::collections::HashSet;

pub const DELIMITER: &str = "____";
pub fn omni_template_to_jinja2(input: &str) -> String {
    regex::Regex::new(r"\$\{([a-z_A-Z.\d]+)\}")
        .unwrap()
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let mut var = caps.get(1).unwrap().as_str().to_owned();
            var = var.replace(".", DELIMITER);
            format!("{{{{ ({}) }}}}", var)
        })
        .to_string()
}

pub fn generate_alias(field_name: &str) -> String {
    let mut alias = field_name.to_string();
    alias = alias.replace(".", DELIMITER);
    alias = alias.replace("[", DELIMITER);
    alias = alias.replace("]", DELIMITER);
    alias = alias.replace(" ", "_");
    alias
}

pub fn get_referenced_variables(text: &str) -> HashSet<String> {
    let mut referenced_fields = HashSet::new();
    let regex = regex::Regex::new(r"\$\{([a-zA-Z0-9_.]+)\}").unwrap();
    for cap in regex.captures_iter(text) {
        if let Some(field) = cap.get(1) {
            referenced_fields.insert(field.as_str().to_string());
        }
    }
    referenced_fields
}
