//! The image tag this process runs as (`main-<sha>`): the deploy identity,
//! what `promote.yaml` pins in the infra values file.
//!
//! Read from `OXY_IMAGE_TAG`, which the image build bakes in
//! (`Dockerfile.prebuilt`); a deployment may also set it. Here, below both
//! `oxy-app` (`/api/version`) and `oxy-telemetry` (`service.version`), so the
//! two name a build the same way.

use std::sync::OnceLock;

/// The variable the image build sets to the tag it pushes.
pub const IMAGE_TAG_VAR: &str = "OXY_IMAGE_TAG";

/// The image tag this process runs as, `None` outside an image (a laptop
/// build, `oxy start`, a CLI download).
pub fn image_tag() -> Option<&'static str> {
    static TAG: OnceLock<Option<String>> = OnceLock::new();
    TAG.get_or_init(|| std::env::var(IMAGE_TAG_VAR).ok().and_then(|v| parse(&v)))
        .as_deref()
}

/// The tag in whatever form a deployment hands over: bare (`main-1bcefe3`),
/// pinned (`main-1bcefe3@sha256:…`) or a full reference
/// (`ghcr.io/oxy-hq/oxygen:main-1bcefe3`).
pub fn parse(raw: &str) -> Option<String> {
    let reference = raw.trim().split('@').next().unwrap_or_default();
    // A `:` after the last `/` separates the tag from the repository; one
    // before it is a registry port.
    let name = reference.rsplit('/').next().unwrap_or_default();
    let tag = name.rsplit_once(':').map_or(name, |(_, tag)| tag);
    (!tag.is_empty()).then(|| tag.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_is_read_from_every_form_a_deployment_hands_over() {
        assert_eq!(parse("main-1bcefe3").as_deref(), Some("main-1bcefe3"));
        assert_eq!(
            parse(" main-1bcefe3@sha256:96efcc34 ").as_deref(),
            Some("main-1bcefe3")
        );
        assert_eq!(
            parse("ghcr.io/oxy-hq/oxygen:main-1bcefe3@sha256:96ef").as_deref(),
            Some("main-1bcefe3")
        );
        assert_eq!(
            parse("localhost:5000/oxygen:main-1bcefe3").as_deref(),
            Some("main-1bcefe3"),
            "a registry port is not the tag"
        );
    }

    #[test]
    fn an_empty_variable_is_no_tag() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
        assert_eq!(parse("@sha256:96ef"), None);
    }
}
