//! Redact auth credentials from git stderr before it reaches logs or
//! user-facing toasts. Git's smart-HTTP backend and `-c` parser can echo
//! the injected `http.extraHeader` value back in error output; we strip
//! the credential before constructing the `OxyError`, leaving the marker
//! prefix in place so the message stays debuggable.

use std::borrow::Cow;

/// Order matters: `http.extraHeader=` is searched first because its value
/// can itself contain `Authorization:` — one greedy redaction covers the
/// whole quoted config snippet instead of three overlapping ones.
const MARKERS: &[&str] = &["http.extraHeader=", "Authorization:", "x-access-token:"];

/// Replace credential markers in `input` with `<marker>[REDACTED]`. The
/// redacted span runs to the next `\n`, `\r`, `'`, or `"` — over-redaction
/// on the same line is preferred to under-redaction. Returns the input
/// unchanged (no allocation) when no marker is found.
pub(crate) fn redact_secrets(input: &str) -> Cow<'_, str> {
    if !MARKERS.iter().any(|m| input.contains(m)) {
        return Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        let next = MARKERS
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, *m)))
            .min_by_key(|&(i, _)| i);

        let Some((idx, marker)) = next else {
            out.push_str(rest);
            return Cow::Owned(out);
        };

        out.push_str(&rest[..idx]);
        out.push_str(marker);
        out.push_str("[REDACTED]");

        let after_marker = idx + marker.len();
        let value_region = &rest[after_marker..];
        let consumed = value_region
            .find(['\n', '\r', '\'', '"'])
            .unwrap_or(value_region.len());
        rest = &value_region[consumed..];
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::redact_secrets;

    #[test]
    fn passes_through_when_no_marker() {
        let s = "fatal: unable to access 'https://github.com/foo/bar/': 404";
        assert!(matches!(redact_secrets(s), std::borrow::Cow::Borrowed(_)));
        assert_eq!(redact_secrets(s), s);
    }

    #[test]
    fn redacts_extra_header_config_line() {
        let s = "error: invalid value 'http.extraHeader=Authorization: Basic eHh4OnRva2VuQUJD' for 'remote.origin'";
        let out = redact_secrets(s);
        assert!(!out.contains("eHh4OnRva2VuQUJD"));
        assert!(!out.contains("Basic"));
        assert!(out.contains("http.extraHeader=[REDACTED]"));
        assert!(out.contains("' for 'remote.origin'"));
    }

    #[test]
    fn redacts_bare_authorization_header() {
        let s = "remote: Authorization: Basic eHh4OnRva2VuQUJD\nfatal: auth failed";
        let out = redact_secrets(s);
        assert!(!out.contains("eHh4OnRva2VuQUJD"));
        assert!(out.contains("Authorization:[REDACTED]"));
        assert!(out.contains("fatal: auth failed"));
    }

    #[test]
    fn redacts_decoded_x_access_token() {
        let s = "got header: x-access-token:ghp_realsecrethere endline";
        let out = redact_secrets(s);
        assert!(!out.contains("ghp_realsecrethere"));
        assert!(out.contains("x-access-token:[REDACTED]"));
    }

    #[test]
    fn redacts_multiple_occurrences() {
        let s = "first 'http.extraHeader=Authorization: Basic AAA' then 'http.extraHeader=Authorization: Basic BBB' done";
        let out = redact_secrets(s);
        assert!(!out.contains("AAA"));
        assert!(!out.contains("BBB"));
        assert_eq!(out.matches("http.extraHeader=[REDACTED]").count(), 2);
    }

    #[test]
    fn stops_at_newline_not_just_quotes() {
        let s = "Authorization: Basic SECRET\nnext line";
        let out = redact_secrets(s);
        assert!(!out.contains("SECRET"));
        assert!(out.contains("\nnext line"));
    }
}
