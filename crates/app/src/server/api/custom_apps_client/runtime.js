/*
 * Oxy custom-app client runtime — injected into every served HTML document,
 * next to `window.__OXY_APP__`.
 *
 * Two jobs, both of which exist so an app author has to do nothing to get them:
 *
 *  1. **Register the platform service worker** (`__oxy/sw.js`) at the app root,
 *     which is what makes a repeat load of a published bundle near-instant.
 *  2. **Instrument the app by default** — SPA pageviews, Core Web Vitals,
 *     engagement time, and uncaught-error counts, posted to `__oxy/beacon`.
 *
 * ## Why instrumentation is not opt-in
 *
 * The server already records one view row per HTML navigation, and for a
 * single-page app that is *one row per session* no matter how much the visitor
 * actually does. "Which screens do people use, and is the app fast for them?"
 * was unanswerable without asking every app author to add a hook to every
 * route — which means it was unanswerable in practice. An app that opts out
 * (`analytics: false` in `oxy-app.json`) still gets the server-side view rows,
 * so the floor never drops below what it was.
 *
 * ## What is deliberately NOT collected
 *
 * No message text, no stack traces, no URL query strings, no element
 * selectors, no free-form strings from the page at all. Paths are same-origin
 * and truncated; error reports carry the constructor name and a count. Every
 * viewer is already an authenticated member whose identity the server stamps
 * on the row — the point of the payload is *what happened*, and none of the
 * above is needed to say that.
 */
(function () {
  "use strict";
  var app = window.__OXY_APP__;
  if (!app || typeof app !== "object") return;

  var base = (app.basePath || "/").replace(/\/*$/, "/");

  // ── service worker ────────────────────────────────────────────────────────
  //
  // Scope is decided HERE, not by the server, because it differs per surface and
  // the HTML is memoized per build rather than per surface:
  //
  //   - Admin host: the page is at `<base>…`, so the worker scopes to `base`
  //     and does not touch the rest of the admin origin.
  //   - Custom-app subdomain: the page is at `/` while the bundle's assets keep
  //     the `<base>` prefix (the host dispatcher passes those through rather
  //     than double-prefixing). Scope must be `/` or the worker never controls
  //     the page; `?base=` tells it where the assets actually live.
  //
  // "Does the current document sit under `base`?" distinguishes the two exactly,
  // needs nothing from the server, and cannot disagree with what the browser is
  // showing. The origin still has the final say: it only sends the
  // `Service-Worker-Allowed` header widening scope to `/` on a subdomain host,
  // so a worker on the admin origin cannot claim the whole SPA even if this
  // computed the wrong answer.
  //
  // `isSecureContext` covers https and localhost, which is exactly where the API
  // exists. Registered after load so it never competes with the critical path
  // for the connection it is meant to make unnecessary.
  if (app.serviceWorker !== false && "serviceWorker" in navigator && window.isSecureContext) {
    var scope = location.pathname.indexOf(base) === 0 ? base : "/";
    var script = base + "__oxy/sw.js?base=" + encodeURIComponent(base);
    var register = function () {
      navigator.serviceWorker.register(script, { scope: scope }).catch(function (err) {
        // Not silent. A worker that will not register costs only the cold path,
        // but "the fast path quietly does not exist here" is exactly the kind of
        // thing that goes unnoticed for a quarter.
        //
        // The known case is the **org-subdomain** surface (`<org>.…/a/<slug>/`):
        // the document lives under `/a/<slug>/` while the bundle's assets keep
        // the `/customer-apps/<org>/<slug>/` prefix, so `scope` computes to `/`
        // and the origin — which only widens `Service-Worker-Allowed` to `/` for
        // a custom-app subdomain — refuses it. Failing closed is right: a `/`
        // scope there would let one app's worker intercept the whole org
        // product origin, SPA included. See `customer-apps-performance.md`
        // § *The client plane* for what a real fix would need.
        if (window.console && console.info) {
          console.info(
            "[oxy] service worker not registered for scope " + scope + " — app runs on the " +
              "network path. " + (err && err.name ? err.name : "registration failed")
          );
        }
      });
    };
    if (document.readyState === "complete") register();
    else window.addEventListener("load", register);
  }

  if (app.analytics === false) return;

  // ── beacon queue ──────────────────────────────────────────────────────────
  var ENDPOINT = base + "__oxy/beacon";
  /** Hard cap per flush — mirrors the server's own limit, so a burst is
   *  dropped here rather than rejected there. */
  var MAX_BATCH = 20;
  var FLUSH_MS = 15000;
  var queue = [];
  var flushTimer = null;

  function push(name, payload) {
    if (queue.length >= MAX_BATCH) return;
    queue.push({ n: name, t: Date.now(), p: payload || {} });
    if (!flushTimer) flushTimer = setTimeout(flush, FLUSH_MS);
  }

  function flush(useBeacon) {
    if (flushTimer) {
      clearTimeout(flushTimer);
      flushTimer = null;
    }
    if (!queue.length) return;
    var body = JSON.stringify({ v: 1, events: queue.splice(0, MAX_BATCH) });
    // `sendBeacon` is the only transport that survives an unload; `keepalive`
    // fetch is the one that reports failures. Use each where it fits.
    if (useBeacon && navigator.sendBeacon) {
      try {
        navigator.sendBeacon(ENDPOINT, new Blob([body], { type: "application/json" }));
        return;
      } catch (e) {
        /* fall through to fetch */
      }
    }
    try {
      fetch(ENDPOINT, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: body,
        credentials: "same-origin",
        keepalive: true
      }).catch(function () {});
    } catch (e) {
      /* analytics must never surface to the app */
    }
  }

  /** Same-origin path only, query and hash dropped, length-bounded. The path is
   *  the whole signal ("which screen"); a query string is where an app puts
   *  record ids and filter values, i.e. the part most likely to be sensitive
   *  and least likely to be useful grouped. */
  function currentPath() {
    var p = location.pathname || "/";
    if (base !== "/" && p.indexOf(base) === 0) p = "/" + p.slice(base.length);
    return p.slice(0, 256);
  }

  // ── SPA pageviews ─────────────────────────────────────────────────────────
  var lastPath = currentPath();
  var routeCount = 1;
  var pageStart = Date.now();
  var maxScroll = 0;

  function onRouteChange() {
    var next = currentPath();
    if (next === lastPath) return;
    // The screen being left is the one whose engagement just ended.
    emitEngagement("route");
    lastPath = next;
    routeCount++;
    pageStart = Date.now();
    maxScroll = 0;
    push("oxy-pageview", { path: next, kind: "spa" });
  }

  // History is patched rather than polled: a router calls `pushState`
  // synchronously and there is no event for it, so the alternatives are a
  // patch or a timer that is always either late or wasteful.
  ["pushState", "replaceState"].forEach(function (method) {
    var original = history[method];
    if (typeof original !== "function") return;
    history[method] = function () {
      var result = original.apply(this, arguments);
      try {
        onRouteChange();
      } catch (e) {}
      return result;
    };
  });
  window.addEventListener("popstate", onRouteChange);
  window.addEventListener("hashchange", onRouteChange);

  // ── engagement ────────────────────────────────────────────────────────────
  window.addEventListener(
    "scroll",
    function () {
      var doc = document.documentElement;
      var height = Math.max(1, doc.scrollHeight - window.innerHeight);
      var pct = Math.round((Math.min(height, window.scrollY) / height) * 100);
      if (pct > maxScroll) maxScroll = pct;
    },
    { passive: true }
  );

  function emitEngagement(reason) {
    var ms = Date.now() - pageStart;
    // Sub-second views are router churn (a redirect, a guard bouncing), not a
    // human looking at a screen. Recording them would put the mode of the
    // duration distribution at zero.
    if (ms < 1000) return;
    // Closing the window as we report it. An ordinary desktop navigation fires
    // `visibilitychange → hidden` AND then `pagehide`, and both call
    // `finalize()`; without this the same stretch of time is pushed twice and
    // every session's engagement total doubles. `emitVitals` has `vitalsSent`
    // and `emitErrors` empties its map — this is the third one, and it was the
    // one missing a guard. Advancing `pageStart` rather than setting a flag is
    // what also makes the tab-away/tab-back case add up instead of overlapping.
    pageStart = Date.now();
    push("oxy-engagement", {
      path: lastPath,
      ms: Math.min(ms, 6 * 60 * 60 * 1000),
      scroll: maxScroll,
      reason: reason
    });
  }

  // ── Core Web Vitals ───────────────────────────────────────────────────────
  // Hand-rolled rather than pulled from `web-vitals`: this script is inlined
  // into every HTML response, so its size is paid on every navigation of every
  // app. The four numbers below are the ones the Activity tab charts, and they
  // are ~40 lines of PerformanceObserver.
  var vitals = {};
  var nav = performance.getEntriesByType && performance.getEntriesByType("navigation")[0];
  if (nav) {
    vitals.ttfb = Math.round(nav.responseStart);
    if (nav.domContentLoadedEventEnd) vitals.dcl = Math.round(nav.domContentLoadedEventEnd);
  }

  function observe(type, handler, opts) {
    try {
      var po = new PerformanceObserver(function (list) {
        list.getEntries().forEach(handler);
      });
      po.observe(Object.assign({ type: type, buffered: true }, opts || {}));
      return po;
    } catch (e) {
      return null;
    }
  }

  observe("paint", function (entry) {
    if (entry.name === "first-contentful-paint") vitals.fcp = Math.round(entry.startTime);
  });
  observe("largest-contentful-paint", function (entry) {
    // LCP is "largest so far" — every entry supersedes the last.
    vitals.lcp = Math.round(entry.startTime);
  });
  observe("layout-shift", function (entry) {
    if (entry.hadRecentInput) return;
    // Stored as CLS × 1000, i.e. an integer of milli-units: the payload column
    // is read by humans and by charts, and a float that arrives as 0.0500000001
    // groups badly and reads worse. `cls: 70` means 0.07 — the "good" threshold
    // is 100. Accumulated by dividing back out first so the rounding does not
    // compound across shifts.
    vitals.cls = Math.round(((vitals.cls || 0) / 1000 + entry.value) * 1000);
  });
  observe("event", function (entry) {
    // INP approximated by the worst interaction latency seen. The real metric
    // is a high percentile of all interactions; the max is the number that
    // actually gets acted on, and it needs no client-side histogram.
    var d = Math.round(entry.duration);
    if (!vitals.inp || d > vitals.inp) vitals.inp = d;
  }, { durationThreshold: 40 });

  var vitalsSent = false;
  function emitVitals() {
    if (vitalsSent) return;
    vitalsSent = true;
    if (Object.keys(vitals).length) push("oxy-web-vitals", vitals);
  }

  // ── errors ────────────────────────────────────────────────────────────────
  // Names and counts, never messages or stacks: a thrown value routinely
  // carries a row id, a SQL fragment, or a user's own input, and this table is
  // read by the app's admin rather than its author.
  var errors = {};
  function noteError(name) {
    var key = String(name || "Error").slice(0, 40);
    errors[key] = (errors[key] || 0) + 1;
  }
  window.addEventListener("error", function (e) {
    noteError(e && e.error && e.error.name);
  });
  window.addEventListener("unhandledrejection", function (e) {
    var r = e && e.reason;
    noteError(r && r.name ? r.name : typeof r);
  });
  function emitErrors() {
    var names = Object.keys(errors);
    if (!names.length) return;
    push("oxy-error", { counts: errors, path: lastPath });
    errors = {};
  }

  // ── lifecycle ─────────────────────────────────────────────────────────────
  // `visibilitychange → hidden` is the only reliable end-of-session signal on
  // mobile; `pagehide` covers desktop navigations that skip it — and an
  // ordinary desktop navigation fires BOTH, in that order. Every emitter below
  // therefore has to be idempotent on its own: `emitVitals` via `vitalsSent`,
  // `emitErrors` by emptying its map, `emitEngagement` by advancing
  // `pageStart`. Draining the queue does not provide that — a flush sends, and
  // sending is exactly what makes a second push a second row.
  function finalize() {
    emitVitals();
    emitErrors();
    emitEngagement("hidden");
    flush(true);
  }
  document.addEventListener("visibilitychange", function () {
    if (document.visibilityState === "hidden") {
      finalize();
      return;
    }
    // Coming back starts a fresh engagement window. Without this, tabbing away
    // and back twice reports the second stretch as everything since the page
    // loaded — so a dashboard left open in a background tab all day reports
    // hours of "engagement" nobody spent looking at it.
    pageStart = Date.now();
  });
  window.addEventListener("pagehide", finalize);

  // A first flush shortly after load carries the vitals for the initial paint
  // even if the visitor never leaves the tab (a dashboard left open all day).
  setTimeout(function () {
    emitVitals();
    flush(false);
  }, 10000);
})();
