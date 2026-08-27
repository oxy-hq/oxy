const STRIPS: &[&str] = &[
    ".agentic.yml",
    ".view.yml",
    ".topic.yml",
    ".app.yml",
    ".procedure.yml",
    ".automation.yml",
    ".airway.yml",
    ".yml",
];

pub fn artifact_name(rel_path: &str) -> String {
    for suffix in STRIPS {
        if let Some(stem) = rel_path.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    rel_path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_suffixes_are_stripped_and_directories_kept() {
        assert_eq!(artifact_name("apps/a.app.yml"), "apps/a");
        assert_eq!(artifact_name("agents/foo.agentic.yml"), "agents/foo");
        assert_eq!(artifact_name("a/b/c.app.yml"), "a/b/c");
        assert_eq!(artifact_name("noext"), "noext");
    }
}
