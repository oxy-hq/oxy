//! Which build this process is: the image tag it ships as, and the commit it was
//! built from.
//!
//! The image tag (`main-<sha>`) is the deploy identity. It is what `promote.yaml`
//! pins in the infra values file, so an operator can hold the two side by side.
//! The Cargo semver is deliberately not reported. It is the last published
//! release, the same string for every image between two releases, so prod
//! answered `0.5.155` while it ran code that shipped in 0.5.156.
//!
//! The tag comes from `OXY_IMAGE_TAG`, which the image build bakes in
//! (`Dockerfile.prebuilt`); a deployment may also set it. A binary that is not an
//! image (a laptop build, `oxy start`, a CLI download) has none, and is named by
//! its commit instead.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The variable the image build sets to the tag it pushes.
const IMAGE_TAG_VAR: &str = "OXY_IMAGE_TAG";

#[derive(Serialize, Deserialize, ToSchema)]
pub struct BuildInfo {
    pub git_commit: String,
    pub git_commit_short: String,
    pub build_timestamp: String,
    pub build_profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_url: Option<String>,
    /// The release this process reports to Sentry, verbatim. Read it rather
    /// than composing it, so it can only drift if the binary does.
    pub sentry_release: String,
}

/// The image tag this process runs as, `None` outside an image.
pub fn image_tag() -> Option<&'static str> {
    static TAG: OnceLock<Option<String>> = OnceLock::new();
    TAG.get_or_init(|| {
        std::env::var(IMAGE_TAG_VAR)
            .ok()
            .and_then(|v| parse_image_tag(&v))
    })
    .as_deref()
}

/// One word for this build: the image tag, or the short commit outside an image
/// (`dev` for a local build).
pub fn build_label() -> String {
    image_tag().map_or_else(|| env!("GIT_HASH").to_string(), str::to_string)
}

/// The tag in whatever form a deployment hands over: bare (`main-1bcefe3`),
/// pinned (`main-1bcefe3@sha256:…`) or a full reference
/// (`ghcr.io/oxy-hq/oxygen:main-1bcefe3`).
fn parse_image_tag(raw: &str) -> Option<String> {
    let reference = raw.trim().split('@').next().unwrap_or_default();
    // A `:` after the last `/` separates the tag from the repository; one
    // before it is a registry port.
    let name = reference.rsplit('/').next().unwrap_or_default();
    let tag = name.rsplit_once(':').map_or(name, |(_, tag)| tag);
    (!tag.is_empty()).then(|| tag.to_string())
}

/// What `/api/version` and `/api/health` say about the build.
pub fn build_info() -> BuildInfo {
    let git_commit = env!("GIT_HASH_LONG").to_string();
    let github_server = env!("GITHUB_SERVER_URL");
    let github_repo = env!("GITHUB_REPOSITORY");
    let github_run_id = env!("GITHUB_RUN_ID");
    let has_repo = !github_server.is_empty() && !github_repo.is_empty();

    // build.rs writes "dev" for a local build; CI writes "unknown" when it has
    // nothing.
    let commit_url = (has_repo && git_commit != "unknown" && git_commit != "dev")
        .then(|| format!("{github_server}/{github_repo}/commit/{git_commit}"));
    let workflow_url = (has_repo && !github_run_id.is_empty() && github_run_id != "unknown")
        .then(|| format!("{github_server}/{github_repo}/actions/runs/{github_run_id}"));

    BuildInfo {
        git_commit,
        git_commit_short: env!("GIT_HASH").to_string(),
        build_timestamp: env!("BUILD_TIMESTAMP").to_string(),
        build_profile: env!("BUILD_PROFILE").to_string(),
        commit_url,
        workflow_url,
        sentry_release: oxy::sentry_config::release_for(env!("GIT_HASH")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_is_read_from_every_form_a_deployment_hands_over() {
        assert_eq!(
            parse_image_tag("main-1bcefe3").as_deref(),
            Some("main-1bcefe3")
        );
        assert_eq!(
            parse_image_tag(" main-1bcefe3@sha256:96efcc34 ").as_deref(),
            Some("main-1bcefe3")
        );
        assert_eq!(
            parse_image_tag("ghcr.io/oxy-hq/oxygen:main-1bcefe3@sha256:96ef").as_deref(),
            Some("main-1bcefe3")
        );
        assert_eq!(
            parse_image_tag("localhost:5000/oxygen:main-1bcefe3").as_deref(),
            Some("main-1bcefe3"),
            "a registry port is not the tag"
        );
    }

    #[test]
    fn an_empty_variable_is_no_tag() {
        assert_eq!(parse_image_tag(""), None);
        assert_eq!(parse_image_tag("   "), None);
        assert_eq!(parse_image_tag("@sha256:96ef"), None);
    }

    #[test]
    fn build_info_reports_the_sentry_release_the_process_uses() {
        assert_eq!(
            build_info().sentry_release,
            oxy::sentry_config::release_for(env!("GIT_HASH"))
        );
    }
}
