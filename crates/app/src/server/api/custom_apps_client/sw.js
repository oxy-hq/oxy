/*
 * Oxy custom-app service worker.
 *
 * Platform-owned, served at `<app-base>/__oxy/sw.js` with a
 * `Service-Worker-Allowed` header widening its scope to the app root. Apps do
 * not write this file and cannot replace it — an app shipping its own worker
 * registers it itself and simply wins the scope race for its own path.
 *
 * ## What it is for
 *
 * A published bundle is content-hashed and immutable. That means the *correct*
 * cache lifetime for almost every byte an app loads is "forever", and the only
 * reason a repeat visit costs anything is that the HTTP cache is not durable,
 * not shared across the app's routes, and re-validates whenever the heuristics
 * feel like it. A worker turns the same immutability into a real precache: the
 * second load of an app is zero network for its critical path.
 *
 * ## The four caches, and why they are separate
 *
 *  - `precache`  — entry assets named by the build's asset manifest, keyed by
 *                  build id. Dropped wholesale when the build changes.
 *  - `runtime`   — hashed assets discovered on demand (lazy route chunks,
 *                  fonts, images). NOT keyed by build: a hashed URL is globally
 *                  unique, so an entry stays valid across publishes and there is
 *                  nothing to invalidate. Trimmed by count.
 *  - `shell`     — the last successful navigation response, used only when the
 *                  network fails. One entry.
 *  - `meta`      — one entry, naming which build the precache holds. **Survives
 *                  both reapers by design**: `dropStalePrecaches` filters on the
 *                  precache prefix and `trimRuntime` evicts the runtime cache's
 *                  oldest entries, which a write-once marker would become. Any
 *                  new cleanup or unregister path has to decide about this one
 *                  deliberately — deleting it costs a near-cold load after the
 *                  next update, and it is the only cache here whose value is
 *                  metadata rather than a response.
 *
 * Keeping runtime out of the build-keyed cache is what makes a publish cheap:
 * a new build re-precaches ~4 entry files instead of re-downloading every lazy
 * chunk the visitor had already accumulated.
 *
 * ## Update policy: no `skipWaiting`
 *
 * A new worker waits for every tab of the app to close before activating.
 *
 * **Not for the reason you would guess, and not the one earlier versions of this
 * comment gave.** They said `activate` deletes the previous build's precache and
 * would strand a tab mid-session on a lazy chunk. Two things make that wrong:
 * lazy chunks are never in the precache (`precacheBuild` caches
 * `manifest.entries`, which is the entry HTML's critical path — everything
 * discovered later lands in the build-agnostic `runtime` cache, which no sweep
 * touches), and the previous generation's precache is dropped *without any
 * activation* on the ordinary publish path, because `syncBuild` ends with
 * `dropStalePrecaches` while old-build tabs are still open. Precache deletion is
 * simply not the hazard.
 *
 * The real reason is narrower and still sufficient: `skipWaiting` +
 * `clients.claim` moves an **already-loaded page onto a worker it did not start
 * under**, swapping the fetch handler — and any change to this file's caching
 * contract — beneath running JavaScript. There is nothing to weigh against that,
 * because waiting costs the design nothing: a publish is picked up immediately
 * regardless. Navigations are network-first, so the new shell arrives at once
 * and references hashed URLs the old worker passes straight through; and the
 * `x-oxy-build` header on that shell lets the *running* worker re-precache the
 * new build in place. Activation is not on the path to being up to date.
 */

/**
 * What this worker controls, from its registration. NOT the same thing as where
 * the app's files live — see `ASSET_BASE`.
 */
const SCOPE = new URL(self.registration.scope).pathname;

/**
 * Where the app's bundle is addressed from, passed as `?base=` on the
 * registration URL.
 *
 * The two differ on the **subdomain** surface, and getting that wrong is the
 * difference between a worker that helps and one that does nothing:
 *
 * - On the admin host the app is at `/customer-apps/<org>/<app>/`, and both the
 *   page and its assets live under it. Scope and base are the same.
 * - On `<org>--<app>.customer-apps…`, the page is at `/` — but the bundle bakes
 *   the subpath into its asset URLs and the host dispatcher deliberately passes
 *   those through rather than double-prefixing them, so assets are *still* at
 *   `/customer-apps/<org>/<app>/…`. Scope has to be `/` (or the worker never
 *   controls the page at all); base stays the subpath.
 *
 * Deriving base from scope would break the first case; deriving scope from base
 * would break the second. So the registrar passes both.
 */
const ASSET_BASE = withTrailingSlash(
  new URL(self.location.href).searchParams.get("base") || SCOPE
);

const PRECACHE_PREFIX = `oxy-precache::${ASSET_BASE}::`;
const RUNTIME_CACHE = `oxy-runtime::${ASSET_BASE}`;
const SHELL_CACHE = `oxy-shell::${ASSET_BASE}`;
/**
 * One entry: which build this worker's precache holds.
 *
 * A cache of its own rather than a row in an existing one, because it has to
 * survive both reapers — `dropStalePrecaches` filters on `PRECACHE_PREFIX`, and
 * `trimRuntime` evicts the oldest entries of `RUNTIME_CACHE`, which a
 * write-once marker would eventually become.
 */
const META_CACHE = `oxy-meta::${ASSET_BASE}`;
/**
 * Cache key for the marker, resolved to an absolute URL rather than left
 * relative.
 *
 * `new Request("/some/path")` resolves against the worker's own location, which
 * works — but it makes the stored key depend on an implicit base, and the Cache
 * API matches on the *resolved* url. Being explicit means the key a write
 * produces is visibly the key a read looks for, in the worker and in a test
 * harness alike. Nothing ever fetches this URL; it exists only as a key.
 */
const LIVE_BUILD_KEY = new URL(`${ASSET_BASE}__oxy/live-build`, self.location.href).href;
const SHELL_KEY = `${ASSET_BASE}__oxy/shell`;
const MANIFEST_URL = `${ASSET_BASE}__oxy/asset-manifest.json`;

function withTrailingSlash(path) {
  return path.endsWith("/") ? path : `${path}/`;
}

/** Schema this worker understands. A worker outlives the page that installed
 *  it, so it is the one consumer that can genuinely be older than its data —
 *  an unrecognised version means "behave as if there were no manifest". */
const SUPPORTED_SCHEMA = 1;

/** Entry cap for the runtime cache. A large SPA accumulates lazy chunks
 *  indefinitely otherwise, and the browser evicts the whole origin's storage
 *  under pressure rather than the oldest entries — losing the precache too. */
const RUNTIME_MAX_ENTRIES = 200;

/** How long a navigation waits for the network before falling back to the
 *  cached shell. Long enough not to fire on an ordinary slow response, short
 *  enough that a dead network doesn't look like a hung page. */
const NAV_TIMEOUT_MS = 3500;

/** Bundle-relative paths this worker must never touch. Data and function calls
 *  are per-user and often streaming; a cached answer would be wrong for the next
 *  viewer and a buffered one would break SSE. Measured against `ASSET_BASE`,
 *  because that is where the app addresses them from on both surfaces. */
const NEVER_HANDLE = ["__oxy/beacon", "fn/", "api/"];

/** URL prefixes (relative to `ASSET_BASE`) whose contents are content-hashed and
 *  may be served cache-first without revalidation. Mirrors
 *  `custom_apps_asset_manifest::is_immutable_asset_path` and the origin's
 *  `immutable` Cache-Control branch — all three must agree. */
const IMMUTABLE_PREFIXES = ["assets/", "_next/static/"];

/**
 * The build generation this worker's precache holds.
 *
 * **Must be re-derived at startup, never assumed.** A worker's global scope is
 * torn down whenever it goes idle (~30s) and rebuilt on the next event, so a
 * module-level value is `null` far more often than it is set — and both readers
 * below take a destructive branch on `null`:
 *
 *  - `dropStalePrecaches` would compute `keep = null`, match *every* generation
 *    including the live one, and delete the precache it is supposed to protect.
 *    An updated worker can wait days for the last tab to close, and is certainly
 *    terminated during that wait — so the instance that finally runs `activate`
 *    is exactly the one with an empty variable, and the first load after an
 *    update would run with no precache at all. That is the load this whole
 *    thing exists for.
 *  - `handleNavigation` would see `served !== currentBuild` on every cold start
 *    and re-run `syncBuild` — a `no-store` manifest fetch plus a re-fetch of
 *    every entry asset — on a large fraction of navigations.
 *
 * Both self-heal, which is why this reads as "extra work per navigation" rather
 * than a broken app. The cache *names* enumerate which generations exist, but
 * they cannot say which one is live — see [`ensureBuildKnown`], which is the
 * function that answers this and the docstring that explains why order is the
 * wrong signal. A durable marker says which; the names only say how many.
 */
let currentBuild = null;

/**
 * In-flight de-duplication for the rehydration, so concurrent events share one
 * read. **Not** a memo: it is cleared on completion, so a lookup that found
 * nothing is retried next time — which is what we want, because "no precache
 * yet" is a state that ends (install is probably running right now).
 */
let buildReady = null;

/** Record which build this worker's precache holds, durably. */
async function recordLiveBuild(buildId) {
  try {
    const meta = await caches.open(META_CACHE);
    await meta.put(new Request(LIVE_BUILD_KEY), new Response(buildId));
  } catch {
    // Losing the marker costs a near-cold load after the next update, not
    // correctness — `ensureBuildKnown` falls back to the unambiguous case.
  }
}

/**
 * Recover `currentBuild` after the global scope was torn down.
 *
 * **Cache-name order cannot answer this**, which is worth spelling out because
 * it looks like it can. Two generations is not an anomaly — it is the *normal*
 * state for the whole waiting period: `install` precaches the new build and
 * creates its cache, while the previous generation survives until `activate`,
 * which by design waits for the last tab to close. `CacheStorage.keys()` yields
 * creation order, so "the first one" is reliably the **superseded** build. An
 * earlier version of this function did exactly that, and so kept the old
 * precache and deleted the one `install` had just built — in precisely the
 * scenario it was added to fix.
 *
 * So the build id is written down (`recordLiveBuild`) rather than inferred. The
 * fallbacks are ordered by how much they can be trusted:
 *
 *  1. the marker, if its precache still exists;
 *  2. otherwise a single generation, which is unambiguous whatever wrote it —
 *     this is also the path for a client that precached before the marker
 *     existed;
 *  3. otherwise **unknown**, so every destructive branch no-ops rather than
 *     guessing. Two generations and no marker is exactly the case where a guess
 *     is a coin flip on the visitor's next load.
 */
function ensureBuildKnown() {
  if (currentBuild) return Promise.resolve(currentBuild);
  if (!buildReady) {
    buildReady = (async () => {
      const names = await caches.keys();
      const generations = names
        .filter((n) => n.startsWith(PRECACHE_PREFIX))
        .map((n) => n.slice(PRECACHE_PREFIX.length));
      if (!generations.length) return null;

      const marked = await readLiveBuild();
      // The `includes` guard matters: a marker whose precache the browser has
      // since evicted would otherwise authorise deleting the generations we do
      // still hold.
      if (marked && generations.includes(marked)) {
        currentBuild = marked;
      } else if (generations.length === 1) {
        currentBuild = generations[0];
      }
      return currentBuild;
    })()
      .catch(() => null)
      .finally(() => {
        buildReady = null;
      });
  }
  return buildReady;
}

async function readLiveBuild() {
  try {
    // `has` first: `caches.open` creates on miss, so a client that precached
    // before the marker existed would otherwise grow an empty `oxy-meta::` cache
    // on its first activate.
    if (!(await caches.has(META_CACHE))) return null;
    const meta = await caches.open(META_CACHE);
    const hit = await meta.match(new Request(LIVE_BUILD_KEY));
    return hit ? (await hit.text()).trim() || null : null;
  } catch {
    return null;
  }
}

// ── install / activate ──────────────────────────────────────────────────────

self.addEventListener("install", (event) => {
  // Precache eagerly, but never let a failure block installation: a worker that
  // fails to install leaves the app with no worker at all, which is strictly
  // worse than a worker with a cold cache.
  event.waitUntil(precacheLatest().catch(() => {}));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      await ensureBuildKnown();
      await dropStalePrecaches();
      // Overlap the worker's own boot with the navigation fetch. Without this a
      // navigation waits for the worker to spin up (tens of ms, more on a cold
      // mobile CPU) BEFORE its network request starts — latency the worker
      // itself introduced. With it the browser issues the navigation request in
      // parallel with worker startup and hands us the result as
      // `event.preloadResponse`. Guarded because Safari lacks it.
      if (self.registration.navigationPreload) {
        await self.registration.navigationPreload.enable();
      }
      await self.clients.claim();
    })()
  );
});

self.addEventListener("message", (event) => {
  // Escape hatch for a host page that wants the pending worker now (e.g. an
  // explicit "reload to update" affordance). Never called automatically — see
  // the update-policy note at the top.
  if (event.data && event.data.type === "OXY_SKIP_WAITING") self.skipWaiting();
});

// ── fetch ───────────────────────────────────────────────────────────────────

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  let url;
  try {
    url = new URL(request.url);
  } catch {
    return;
  }
  if (url.origin !== self.location.origin) return;
  if (!url.pathname.startsWith(SCOPE)) return;
  // The platform's own API is never the app's to cache, whatever the scope is.
  // On a subdomain the scope is the whole origin, so `/api/*` lands inside it.
  if (url.pathname === "/api" || url.pathname.startsWith("/api/")) return;

  const rel = relativeToBase(url.pathname);
  if (rel !== null && NEVER_HANDLE.some((p) => rel.startsWith(p))) return;
  // An event stream must reach the network untouched, whatever its path.
  if ((request.headers.get("accept") || "").includes("text/event-stream")) return;
  // `cache: 'no-store'` is an explicit caller instruction; honour it.
  if (request.cache === "no-store" || request.cache === "reload") return;

  if (request.mode === "navigate") {
    event.respondWith(handleNavigation(event, request));
    return;
  }
  if (rel !== null && isImmutable(rel)) {
    event.respondWith(handleImmutableAsset(request));
  }
  // Everything else in scope falls through to the network untouched.
});

/**
 * Navigation: network-first with a timeout, cached shell as the offline
 * fallback.
 *
 * Network-first rather than stale-while-revalidate because the shell is the
 * document that names which build's chunks to load. Serving a stale one and
 * revalidating behind it would render the previous build for one full load
 * after every publish — the exact staleness a publish is supposed to end.
 */
async function handleNavigation(event, request) {
  try {
    // `event.preloadResponse` is the request the browser started in parallel
    // with worker boot (see `navigationPreload` in activate). It is a promise
    // resolving to that Response, or to `undefined` when preload is off or
    // unsupported — in which case we issue the fetch ourselves. Racing the whole
    // thing against `NAV_TIMEOUT_MS` keeps the "dead network → cached shell"
    // fallback: a hung preload trips the timeout exactly as a hung fetch did.
    const response = await withTimeout(
      Promise.resolve(event.preloadResponse).then((preloaded) => preloaded || fetch(request)),
      NAV_TIMEOUT_MS
    );
    if (isCacheableShell(response)) {
      const copy = response.clone();
      event.waitUntil(
        (async () => {
          const cache = await caches.open(SHELL_CACHE);
          await cache.put(SHELL_KEY, copy);
        })()
      );
      // The origin stamps the live build on every shell. If it moved, refresh
      // the precache in place rather than waiting for this worker to be
      // replaced — the new build's entry files are what the next load needs.
      const served = response.headers.get("x-oxy-build");
      if (served) {
        event.waitUntil(
          // Rehydrate BEFORE comparing: on a cold start `currentBuild` is null,
          // and comparing against it would re-precache the build we already
          // hold on most navigations.
          ensureBuildKnown().then((known) =>
            served === known ? undefined : syncBuild(served)
          )
        );
      }
    }
    return response;
  } catch {
    const cached = await caches.match(SHELL_KEY, { cacheName: SHELL_CACHE });
    if (cached) return cached;
    throw new Error("offline and no cached shell");
  }
}

/**
 * A content-hashed asset: precache, then runtime cache, then network. A hit
 * never revalidates — that is the whole point of a hashed URL, and it is what
 * the origin's `immutable` policy already promises for the same paths.
 */
async function handleImmutableAsset(request) {
  const hit = await caches.match(request, { ignoreSearch: false });
  if (hit) return hit;
  const response = await fetch(request);
  if (isCacheableAsset(response)) {
    const copy = response.clone();
    const cache = await caches.open(RUNTIME_CACHE);
    await cache.put(request, copy);
    trimRuntime().catch(() => {});
  }
  return response;
}

// ── precache ────────────────────────────────────────────────────────────────

/** Read the manifest and precache the build it names. */
async function precacheLatest() {
  const manifest = await fetchManifest();
  if (!manifest) return;
  await precacheBuild(manifest);
}

/** Re-point the precache at `buildId` after the origin reported a new build. */
async function syncBuild(buildId) {
  if (buildId === (await ensureBuildKnown())) return;
  const manifest = await fetchManifest();
  if (!manifest || manifest.buildId !== buildId) return;
  await precacheBuild(manifest);
  await dropStalePrecaches();
}

async function fetchManifest() {
  try {
    // `no-store` on the manifest itself: it is the document that tells us what
    // is fresh, so reading a cached copy of it would defeat the check.
    const response = await fetch(MANIFEST_URL, { cache: "no-store", credentials: "same-origin" });
    if (!response.ok) return null;
    const manifest = await response.json();
    if (!manifest || manifest.schemaVersion !== SUPPORTED_SCHEMA) return null;
    if (typeof manifest.buildId !== "string" || !Array.isArray(manifest.entries)) return null;
    return manifest;
  } catch {
    return null;
  }
}

async function precacheBuild(manifest) {
  const cache = await caches.open(PRECACHE_PREFIX + manifest.buildId);
  const urls = manifest.entries
    .map((e) => e && e.path)
    .filter((p) => typeof p === "string" && p.length > 0)
    .map((p) => ASSET_BASE + p.replace(/^\/+/, ""));
  // Individually, not `cache.addAll`: one 404 (a manifest naming a file a later
  // rollback removed) would otherwise reject the whole batch and leave the
  // precache empty.
  await Promise.all(
    urls.map(async (u) => {
      try {
        const response = await fetch(u, { credentials: "same-origin" });
        if (isCacheableAsset(response)) await cache.put(u, response);
      } catch {
        /* one cold entry, not a failed install */
      }
    })
  );
  currentBuild = manifest.buildId;
  // Durably, so a torn-down global can tell which generation is ours without
  // guessing from cache-name order — but only if this precache is worth being
  // trusted as the live one.
  //
  // Every asset fetch above swallows its own failure, so "manifest fetched, all
  // entries failed" produces an empty cache that would otherwise be marked live
  // — and a later fresh-global `activate` would trust the marker and sweep a
  // *populated* generation in its favour. Declining to mark it falls back to
  // "two generations, no marker → decline", which is the better outcome.
  //
  // `urls.length === 0` is the case that must still be marked: a bundle whose
  // entry HTML inlines everything has nothing to precache, and an empty cache is
  // the correct and complete result for it. The distinction is "intended to
  // cache nothing" versus "intended to cache something and got nothing".
  const stored = (await cache.keys()).length;
  if (stored > 0 || urls.length === 0) await recordLiveBuild(manifest.buildId);
}

/**
 * Delete every precache generation but the live one.
 *
 * A no-op when the live generation is unknown, rather than a full sweep. "I do
 * not know what to keep" and "keep nothing" are opposite instructions, and the
 * cost of confusing them is asymmetric: skipping a sweep leaves one superseded
 * generation resident until the next activation, while sweeping blind throws
 * away the precache the next navigation is about to use.
 */
async function dropStalePrecaches() {
  if (!currentBuild) return;
  const keep = PRECACHE_PREFIX + currentBuild;
  const names = await caches.keys();
  await Promise.all(
    names
      .filter((n) => n.startsWith(PRECACHE_PREFIX) && n !== keep)
      .map((n) => caches.delete(n))
  );
}

/** Bound the runtime cache. Oldest-inserted first — the Cache API preserves
 *  insertion order in `keys()`, which is the closest thing to an LRU available
 *  without keeping a side index. */
async function trimRuntime() {
  const cache = await caches.open(RUNTIME_CACHE);
  const keys = await cache.keys();
  if (keys.length <= RUNTIME_MAX_ENTRIES) return;
  await Promise.all(keys.slice(0, keys.length - RUNTIME_MAX_ENTRIES).map((k) => cache.delete(k)));
}

// ── predicates ──────────────────────────────────────────────────────────────

/** A pathname expressed relative to the bundle root, or `null` when it is not
 *  inside it — which happens on the subdomain surface for the app's own
 *  client-side routes, and those are navigations rather than assets. */
function relativeToBase(pathname) {
  return pathname.startsWith(ASSET_BASE) ? pathname.slice(ASSET_BASE.length) : null;
}

function isImmutable(rel) {
  return IMMUTABLE_PREFIXES.some((p) => rel.startsWith(p));
}

/**
 * Only a same-origin 200 that was not a redirect may be stored.
 *
 * `response.redirected` is the load-bearing one: an expired session turns every
 * request into a 302 to `/login`, and the login page answers 200. Storing that
 * would pin a login page at an asset URL for the life of the cache.
 */
function isCacheableAsset(response) {
  return !!response && response.ok && response.type === "basic" && !response.redirected;
}

function isCacheableShell(response) {
  if (!isCacheableAsset(response)) return false;
  const type = response.headers.get("content-type") || "";
  return type.includes("text/html");
}

function withTimeout(promise, ms) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timeout")), ms);
    promise.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      }
    );
  });
}
