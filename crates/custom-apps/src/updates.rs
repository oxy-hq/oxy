//! How a long-open custom-app tab picks up a new build.
//!
//! A custom app is a single-page app: a tab loads one build and keeps running
//! it until something reloads the document. An iPad left on a checklist, a wall
//! kiosk or an installed PWA can run the build it loaded for days, across any
//! number of publishes. The platform-injected `runtime.js` closes that gap: it
//! polls `__oxy/build`, announces a new build to the app, and — only in `auto`
//! mode — reloads at a safe moment. See `runtime.js` § *updates* and
//! `internal-docs/customer-apps-performance.md` § *Open tabs and new builds*.
//!
//! This module owns the server half: the process-wide mode, how an app's
//! manifest narrows it, and the `/fn` build handshake headers.

use std::sync::OnceLock;

/// Environment variable that sets the platform-wide update mode.
pub const UPDATE_MODE_ENV: &str = "OXY_CUSTOM_APP_UPDATES";

/// Response header on every `/fn` answer: the `app_builds.id` the function ran
/// against. A client that sees a different id from the one it loaded knows a
/// publish happened before its own poll does.
pub const LIVE_BUILD_HEADER: &str = "x-oxy-live-build";

/// Request header the SDK's `useFunction` sends: the `app_builds.id` the
/// calling document was served. Absent means *unknown* — an app built on an
/// older SDK — and is never read as stale.
pub const CLIENT_BUILD_HEADER: &str = "x-oxy-client-build";

/// What the injected runtime does about a new build.
///
/// Process-wide, so it cannot vary per viewer and the memoized HTML key stays
/// valid (`custom_apps_html_cache`). Changing it is a pod restart — the kill
/// switch is `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// Nothing: no probe, no announcement, no stale-chunk recovery.
    Off,
    /// Detect and announce (`oxy:update-available`, an `oxy-update` beacon
    /// row), recover a broken lazy chunk, never reload otherwise. The default:
    /// reloading another author's app is a behaviour change for every app at
    /// once, so it ships measured before it ships acting.
    Observe,
    /// Observe, plus reload at a safe moment no sooner than 60 s after
    /// detection, unless the app deferred or cancelled.
    Auto,
}

impl UpdateMode {
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateMode::Off => "off",
            UpdateMode::Observe => "observe",
            UpdateMode::Auto => "auto",
        }
    }

    /// Parse the env value. Anything unrecognised is the default rather than an
    /// error: a typo in a kill switch must not take the runtime somewhere
    /// nobody chose, and `observe` never reloads anything.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => UpdateMode::Off,
            Some("auto") => UpdateMode::Auto,
            Some("observe") | Some("") | None => UpdateMode::Observe,
            Some(other) => {
                tracing::warn!(
                    "{UPDATE_MODE_ENV}={other:?} is not off|observe|auto; using observe"
                );
                UpdateMode::Observe
            }
        }
    }
}

/// The platform-wide mode, read once per process.
pub fn platform_mode() -> UpdateMode {
    static MODE: OnceLock<UpdateMode> = OnceLock::new();
    *MODE.get_or_init(|| UpdateMode::parse(std::env::var(UPDATE_MODE_ENV).ok().as_deref()))
}

/// An app's own update policy, from `performance.autoUpdate` in its manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppUpdatePolicy {
    /// Said nothing: the platform mode applies.
    #[default]
    Platform,
    /// `autoUpdate: true` — reload at a safe moment even while the platform is
    /// in `observe`.
    Auto,
    /// `autoUpdate: false` — never reload automatically; detection, the event
    /// and stale-chunk recovery still run.
    Never,
    /// `autoUpdate: "prompt"` — the SDK shows an "Update ready" control, and
    /// the runtime escalates to `auto` 24 h after detection so an unattended
    /// kiosk still converges.
    Prompt,
}

/// The `updates` value injected into `window.__OXY_APP__`.
///
/// `off` at the platform level wins over everything: it is the kill switch,
/// and an app must not be able to opt back in past it.
pub fn runtime_mode(platform: UpdateMode, app: AppUpdatePolicy) -> &'static str {
    if platform == UpdateMode::Off {
        return UpdateMode::Off.as_str();
    }
    match app {
        AppUpdatePolicy::Platform => platform.as_str(),
        AppUpdatePolicy::Auto => UpdateMode::Auto.as_str(),
        AppUpdatePolicy::Never => UpdateMode::Observe.as_str(),
        AppUpdatePolicy::Prompt => "prompt",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_modes_and_defaults_to_observe() {
        assert_eq!(UpdateMode::parse(Some("off")), UpdateMode::Off);
        assert_eq!(UpdateMode::parse(Some(" AUTO ")), UpdateMode::Auto);
        assert_eq!(UpdateMode::parse(Some("observe")), UpdateMode::Observe);
        assert_eq!(UpdateMode::parse(None), UpdateMode::Observe);
        assert_eq!(UpdateMode::parse(Some("")), UpdateMode::Observe);
        // A typo never lands somewhere that reloads.
        assert_eq!(UpdateMode::parse(Some("on")), UpdateMode::Observe);
    }

    #[test]
    fn the_platform_kill_switch_beats_the_app() {
        for app in [
            AppUpdatePolicy::Platform,
            AppUpdatePolicy::Auto,
            AppUpdatePolicy::Never,
            AppUpdatePolicy::Prompt,
        ] {
            assert_eq!(runtime_mode(UpdateMode::Off, app), "off");
        }
    }

    #[test]
    fn an_app_policy_narrows_or_opts_in_under_observe_and_auto() {
        let o = UpdateMode::Observe;
        assert_eq!(runtime_mode(o, AppUpdatePolicy::Platform), "observe");
        assert_eq!(runtime_mode(o, AppUpdatePolicy::Auto), "auto");
        assert_eq!(runtime_mode(o, AppUpdatePolicy::Prompt), "prompt");
        let a = UpdateMode::Auto;
        assert_eq!(runtime_mode(a, AppUpdatePolicy::Platform), "auto");
        assert_eq!(runtime_mode(a, AppUpdatePolicy::Never), "observe");
    }
}
