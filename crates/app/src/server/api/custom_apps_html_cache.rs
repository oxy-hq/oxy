//! Memoized HTML entry documents for the custom-app serve path.
//!
//! Every navigation to a custom app runs the same pure transform over the same
//! stored bytes: rewrite the bundle's build-time base path to the serve-time
//! one, splice `window.__OXY_APP__` and the client runtime into `<head>`, hash
//! the result into a weak ETag, and render the manifest's entry list into a
//! `Link` preload header. None of it depends on the *viewer* — the injected
//! identity is app-level, never user-level, which is exactly the property that
//! lets the origin treat the body as shareable (see `cache_control_for`'s note
//! on why the *cookie*, not the body, is what makes the response `private`).
//!
//! So the work is per-build, and it was being redone per request. On the
//! navigation path that is a JSON serialize, two full copies of the document,
//! a byte scan for `</head>`, and a hash over the result — small individually,
//! and directly in front of the first paint of every app open.
//!
//! ## Why a separate cache from the bundle LRU
//!
//! `custom_apps_bundle_cache` holds *stored* bytes; this holds *rendered* ones.
//! Mixing them would put a derived artifact under a key that names a stored
//! object, and the derivation depends on things the object key doesn't
//! mention — the org and app slugs, which appear in the rewritten base path
//! and change on a rename.
//!
//! ## Invalidation
//!
//! There is none, and there does not need to be. The key names the `build_id`,
//! both slugs and the app environment, so every input to the transform is in
//! the key:
//!
//! - a publish, promote, or rollback moves the channel to a different
//!   `build_id` → different key;
//! - a rename changes a slug → different key;
//! - staging and production serving the same build inject a different
//!   `environment` → different key;
//! - the transform itself changes only when the binary does → a deploy is a
//!   new process with an empty cache.
//!
//! The one input not in the key is the *runtime config struct*, whose fields
//! are all derived from the app row and the slugs. A future field that varies
//! by viewer would break that — and would break the `private`-because-of-the-
//! cookie reasoning above at the same time, so it needs to be caught there
//! regardless.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use axum::body::Bytes;
use lru::LruCache;
use parking_lot::Mutex;
use uuid::Uuid;

use super::custom_apps_precompress as precompress;

/// Entry cap. Each entry is one rendered HTML document — a few KiB for an SPA
/// shell, tens for a static export's deep page. Bounded by count because the
/// population is bounded by construction: an app has one shell plus however
/// many `.html` files a static export ships, and only the ones people actually
/// open land here.
const MAX_ENTRIES: usize = 512;

/// A fully rendered entry document plus the response metadata derived from it.
#[derive(Clone)]
pub struct RenderedHtml {
    /// Final bytes, after base-path rewrite and injection.
    pub body: Bytes,
    /// Weak ETag over [`Self::body`]. Weak because the body is a transform of
    /// the stored object rather than the object itself.
    pub etag: String,
    /// `Link` header value hinting the build's entry assets, or `None` when the
    /// build has no manifest (published before manifests existed, or a
    /// local-folder source).
    pub link: Option<String>,
    /// The brotli form of [`Self::body`], filled once, off the request path.
    /// Shared (`Arc`) across the clones the LRU hands out, so the one
    /// compression a first miss schedules is visible to every later hit.
    pub br: Arc<BrotliVariant>,
}

impl RenderedHtml {
    pub fn new(body: Bytes, etag: String, link: Option<String>) -> Self {
        Self {
            body,
            etag,
            link,
            br: Arc::default(),
        }
    }
}

/// A lazily-filled brotli copy of one rendered document.
///
/// The transform is memoised per build, but the *compression* of its output
/// used to run per navigation inside `CompressionLayer`, on the critical path
/// of every app open. Now it runs once: the first request that could use it
/// [`schedule`](Self::schedule)s the work onto the blocking pool and is itself
/// answered with the identity bytes, and every later one reads the result with
/// [`get`](Self::get). A result of "brotli did not shrink it" is remembered too,
/// so a document that never compresses is not retried per request.
#[derive(Default)]
pub struct BrotliVariant {
    /// `Some(None)` = tried, did not pay (or failed); never tried again.
    ready: OnceLock<Option<Bytes>>,
    scheduled: AtomicBool,
}

impl BrotliVariant {
    /// The compressed bytes, or `None` while pending or when brotli does not
    /// pay for this document.
    pub fn get(&self) -> Option<Bytes> {
        self.ready.get().cloned().flatten()
    }

    /// Start the compression if nobody has. Idempotent and never blocks: with
    /// no Tokio runtime (a plain `#[test]`) it does nothing and a later call may
    /// try again.
    pub fn schedule(self: &Arc<Self>, body: &Bytes) {
        if self.ready.get().is_some() || self.scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.scheduled.store(false, Ordering::Release);
            return;
        };
        let (this, body) = (Arc::clone(self), body.clone());
        runtime.spawn_blocking(move || {
            let _ = this
                .ready
                .set(precompress::compress(&body).map(Bytes::from));
        });
    }
}

type Cache = Mutex<LruCache<String, RenderedHtml>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(MAX_ENTRIES).expect("MAX_ENTRIES > 0"),
        ))
    })
}

/// Every input to the transform, in the key. See the module's invalidation
/// note for why that is the whole invalidation story.
///
/// `environment` is one of them: the rendered document injects it
/// (`window.__OXY_APP__.environment`), and staging and production serve the
/// **same** build — so without it one environment's render would be handed to
/// the other.
#[derive(Clone, Copy, Debug)]
pub struct HtmlKey<'a> {
    pub app_id: Uuid,
    pub build_id: &'a str,
    pub object_key: &'a str,
    pub org_slug: &'a str,
    pub app_slug: &'a str,
    pub environment: &'a str,
}

fn key(k: &HtmlKey<'_>) -> String {
    format!(
        "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
        k.app_id, k.build_id, k.object_key, k.org_slug, k.app_slug, k.environment
    )
}

pub fn get(k: &HtmlKey<'_>) -> Option<RenderedHtml> {
    cache().lock().get(&key(k)).cloned()
}

pub fn put(k: &HtmlKey<'_>, rendered: RenderedHtml) {
    cache().lock().put(key(k), rendered);
}

/// Drop everything. Only for tests and for the rare mutation that changes the
/// transform without changing any key component — there is none today, and a
/// new one should be questioned rather than served by calling this.
#[cfg(test)]
pub fn clear() {
    cache().lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(app, build, page, org, app slug, environment)`.
    fn k<'a>(
        app_id: Uuid,
        build_id: &'a str,
        object_key: &'a str,
        org_slug: &'a str,
        app_slug: &'a str,
        environment: &'a str,
    ) -> HtmlKey<'a> {
        HtmlKey {
            app_id,
            build_id,
            object_key,
            org_slug,
            app_slug,
            environment,
        }
    }

    fn rendered(body: &str) -> RenderedHtml {
        RenderedHtml::new(
            Bytes::from(body.to_string()),
            format!("W/\"{}\"", body.len()),
            None,
        )
    }

    #[test]
    fn round_trips_on_an_identical_key() {
        clear();
        let app = Uuid::new_v4();
        put(
            &k(app, "b1", "index.html", "acme", "sales", "production"),
            rendered("<html>1"),
        );
        let hit = get(&k(app, "b1", "index.html", "acme", "sales", "production")).expect("hit");
        assert_eq!(hit.body, Bytes::from_static(b"<html>1"));
    }

    /// Each of the five key components must be able to miss on its own — a
    /// promote, a rename, a second page, and a different app all have to see
    /// their own render.
    #[test]
    fn every_key_component_separates_entries() {
        clear();
        let app = Uuid::new_v4();
        let other_app = Uuid::new_v4();
        put(
            &k(app, "b1", "index.html", "acme", "sales", "production"),
            rendered("<html>1"),
        );

        assert!(
            get(&k(app, "b2", "index.html", "acme", "sales", "production")).is_none(),
            "build"
        );
        assert!(
            get(&k(app, "b1", "about.html", "acme", "sales", "production")).is_none(),
            "page"
        );
        assert!(
            get(&k(app, "b1", "index.html", "acme2", "sales", "production")).is_none(),
            "org slug"
        );
        assert!(
            get(&k(app, "b1", "index.html", "acme", "sales2", "production")).is_none(),
            "app slug"
        );
        assert!(
            get(&k(
                other_app,
                "b1",
                "index.html",
                "acme",
                "sales",
                "production"
            ))
            .is_none(),
            "app id"
        );
        // Staging and production serve the same build, so this is the only
        // component that tells their renders apart.
        assert!(
            get(&k(app, "b1", "index.html", "acme", "sales", "staging")).is_none(),
            "environment"
        );
    }

    /// The key is built by concatenation, so a component whose value contains
    /// the joiner could otherwise forge another entry's key. The separator is a
    /// control character no slug, uuid, or object path can contain — but a
    /// build id is engineer-supplied, so pin the property rather than trusting
    /// the validator two modules away.
    #[test]
    fn components_cannot_bleed_into_each_other() {
        clear();
        let app = Uuid::new_v4();
        put(
            &k(app, "b1", "a/index.html", "acme", "sales", "production"),
            rendered("<html>A"),
        );
        // Same concatenation if the separator were "/" — must still miss.
        assert!(get(&k(app, "b1/a", "index.html", "acme", "sales", "production")).is_none());
    }

    /// The variant is shared by every clone of an entry — the LRU hands out
    /// clones, and a compression scheduled from one must be visible to the next.
    #[tokio::test]
    async fn the_brotli_variant_is_filled_once_and_shared_across_clones() {
        clear();
        let app = Uuid::new_v4();
        let key = k(app, "b1", "index.html", "acme", "sales", "production");
        let body = "<html><body>".to_string() + &"<p>hello custom app</p>".repeat(200);
        put(&key, rendered(&body));

        let first = get(&key).expect("hit");
        assert!(
            first.br.get().is_none(),
            "nothing is compressed before it is asked for"
        );
        first.br.schedule(&first.body);
        first.br.schedule(&first.body); // idempotent

        let second = get(&key).expect("hit");
        let mut compressed = None;
        for _ in 0..2000 {
            compressed = second.br.get();
            if compressed.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let compressed = compressed.expect("the scheduled compression must land");
        assert!(compressed.len() < first.body.len());
        assert_ne!(compressed, first.body);
    }

    /// A document brotli cannot shrink is remembered as such: `get` stays
    /// `None` and `schedule` does not run it again.
    #[tokio::test]
    async fn a_document_brotli_cannot_shrink_is_not_retried() {
        let variant = Arc::new(BrotliVariant::default());
        let tiny = Bytes::from_static(b"x");
        variant.schedule(&tiny);
        for _ in 0..2000 {
            if variant.ready.get().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(variant.ready.get(), Some(&None));
        assert!(variant.get().is_none());
    }

    /// Outside a runtime `schedule` must not panic, and must leave the door
    /// open for a later call that has one.
    #[test]
    fn scheduling_without_a_runtime_is_a_no_op() {
        let variant = Arc::new(BrotliVariant::default());
        variant.schedule(&Bytes::from_static(b"<html></html>"));
        assert!(variant.ready.get().is_none());
        assert!(!variant.scheduled.load(Ordering::Acquire));
    }
}
