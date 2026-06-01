/// URL-safe identifier derived from a human-readable name.
///
/// Strategy:
/// - ASCII-lower everything.
/// - Any run of non-alphanumeric chars (including non-ASCII letters,
///   punctuation, whitespace) collapses to a single `-`.
/// - Leading and trailing dashes are stripped.
/// - Output is capped at 60 chars (DNS labels are 63; we leave headroom
///   for an 8-char collision-dedup suffix the caller may append).
/// - Empty input yields `"app"` so the caller never has to handle the
///   empty-string corner case explicitly.
///
/// Used by the customer-apps registry to auto-derive `apps.slug` from
/// `apps.name` on create and during the slug-backfill migration.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = true; // strip leading dashes
    for ch in name.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        return "app".to_string();
    }
    if out.len() > 60 {
        out.truncate(60);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn common_names() {
        assert_eq!(slugify("Acme Analytics"), "acme-analytics");
        assert_eq!(slugify("Acme  Analytics  "), "acme-analytics");
        assert_eq!(slugify("---weird---"), "weird");
        assert_eq!(slugify("UPPER lower"), "upper-lower");
        assert_eq!(slugify("Spaces / and / slashes"), "spaces-and-slashes");
    }

    #[test]
    fn empty_yields_app() {
        assert_eq!(slugify(""), "app");
        assert_eq!(slugify("///"), "app");
        assert_eq!(slugify("---"), "app");
    }

    #[test]
    fn caps_60_no_trailing_dash() {
        let long = "a".repeat(80);
        let result = slugify(&long);
        assert_eq!(result.len(), 60);
        assert!(!result.ends_with('-'));
    }

    #[test]
    fn non_ascii_treated_as_separator() {
        // We don't romanise: the admin can rename if they want a better slug.
        assert_eq!(slugify("Café — Daily"), "caf-daily");
        assert_eq!(slugify("日本語"), "app"); // entirely non-ASCII → empty → fallback
    }

    #[test]
    fn digits_kept() {
        assert_eq!(slugify("Q3 Report 2026"), "q3-report-2026");
    }
}
