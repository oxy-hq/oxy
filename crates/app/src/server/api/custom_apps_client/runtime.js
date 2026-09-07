/*
 * Oxy custom-app client runtime — injected into every served HTML document,
 * next to `window.__OXY_APP__`.
 *
 * Three jobs, all of which exist so an app author has to do nothing to get them:
 *
 *  1. **Register the platform service worker** (`__oxy/sw.js`) at the app root,
 *     which is what makes a repeat load of a published bundle near-instant.
 *  2. **Report platform health** — did the app mount (`oxy-app-ready`), and did
 *     it throw (`oxy-error`).
 *  3. **Instrument the app by default** — SPA pageviews, Core Web Vitals and
 *     engagement time, posted to `__oxy/beacon`.
 *
 * ## Two categories, one flag
 *
 * `analytics: false` in `oxy-app.json` silences (3) and nothing else. (2) is
 * platform health and is never opt-out-able, for the same reason the
 * server-side view row never was: an operator cannot answer "is this app up"
 * by editing someone else's bundle, and an app that is down is exactly the app
 * least likely to have opted in.
 *
 * The split is drawn where it is because health costs no privacy. `app-ready`
 * is a boolean — it carries no path and says only that the bundle rendered
 * something. Errors cross the line as *constructor names and counts* only;
 * their message, stack and screen path are free text from the page and follow
 * the opt-out with everything else. Neither exempt signal describes what a
 * viewer did. `oxy-pageview` does, which is why it sits on the analytics side
 * despite being useful here: the server-side view row is a sufficient
 * denominator for the white-screen signal without it.
 *
 * ## What is and is not collected
 *
 * Analytics events carry no free-form strings from the page: no query strings,
 * no element selectors, no DOM text. Paths are same-origin and truncated.
 *
 * **Errors are the exception, and it is deliberate.** Since 2026-09 an error
 * report carries its message and stack as well as its name, because names and
 * counts alone made a white-screened app report `{TypeError: 3}` and nothing
 * anyone could act on. That text goes to a *separate* server-side table with
 * shorter retention behind the app-admin gate — never into the analytics rows
 * this runtime otherwise produces — and it is sent only when the app has NOT
 * opted out. Bounds live in the error section below: dedup per session by
 * stack, a cap per flush, truncation before the wire.
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

  // ── the opt-out split ─────────────────────────────────────────────────────
  //
  // `analytics: false` used to return here and silence the client runtime
  // entirely. It now silences the *author's product analytics* only —
  // pageviews, web vitals, engagement. Two signals stay on for every app:
  //
  //   `oxy-app-ready`  did this app actually mount?
  //   `oxy-error`      did it throw? (counts only — the message/stack detail
  //                    follows the opt-out, see `noteError`)
  //
  // Those are platform health, not product analytics, and they are exempt for
  // the same reason the server-side view row always was: an operator cannot
  // fix "is this app up" by editing someone else's bundle. They also say
  // nothing about what a viewer did — `app-ready` is a boolean and carries no
  // path, and errors are names and counts. `oxy-pageview` deliberately stays
  // opt-out-able: a list of screen paths IS product analytics, and the
  // server-side view row is a sufficient denominator without it.
  var analyticsEnabled = app.analytics !== false;

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

  // ── app-ready (health signal, never opt-out-able) ─────────────────────────
  //
  // THE white-screen detector, and the reason a status code is not enough. A
  // custom-app host answers 200 with the SPA shell for every path — so a bundle
  // that throws on boot, or whose main chunk 404s, produces a perfect 200 and a
  // blank page. The server cannot tell those apart. The browser can: pair
  // "a shell was served" (the server-side view row, which no app can opt out
  // of) with "the app mounted", and the *absence* of the second is the outage.
  //
  // Absence is the signal, so this deliberately never sends a negative. A
  // "failed to mount" beacon would need the page to survive long enough to send
  // it, which is exactly what a hard boot failure does not do.
  var readySent = false;
  function markReady(how) {
    if (readySent) return;
    readySent = true;
    push("oxy-app-ready", { how: how });
    // Sent immediately rather than on the 15s timer: a boot failure that takes
    // the tab with it must not also take the proof that boot succeeded.
    flush(false);
  }

  // Explicit opt-in for an app that knows better than the heuristic — a bundle
  // that renders a deliberate splash, or mounts into a shadow root.
  window.__oxyAppReady = function () {
    try {
      markReady("explicit");
    } catch (e) {}
  };

  /** Has anything with layout rendered? Bounded scan: it returns on the first
   *  laid-out element, so a mounted app costs a handful of iterations, and an
   *  unmounted one has almost no DOM to walk. The cap is for the pathological
   *  middle — a huge DOM of zero-height nodes — where this must not become the
   *  jank it is measuring. */
  function looksMounted() {
    var root = document.body;
    if (!root) return false;
    var kids = root.querySelectorAll("*");
    var limit = Math.min(kids.length, 200);
    for (var i = 0; i < limit; i++) {
      var el = kids[i];
      var tag = el.tagName;
      if (tag === "SCRIPT" || tag === "STYLE" || tag === "LINK" || tag === "NOSCRIPT") continue;
      try {
        if (el.getBoundingClientRect().height > 0) return true;
      } catch (e) {}
    }
    return false;
  }

  // Polled rather than observed: MutationObserver fires on the first DOM write,
  // which for a framework is the empty root container — earlier than "mounted"
  // and it would call a white screen healthy. These checks bracket the range a
  // real app boots in; past the last one it is not booting.
  function checkReady() {
    if (readySent) return;
    if (looksMounted()) markReady("auto");
  }
  [0, 1000, 3000, 10000].forEach(function (delay) {
    setTimeout(checkReady, delay);
  });
  if (document.readyState === "complete") checkReady();
  else window.addEventListener("load", checkReady);

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
    if (analyticsEnabled) push("oxy-pageview", { path: next, kind: "spa" });
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
    if (!analyticsEnabled) return;
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
  var nav =
    analyticsEnabled &&
    performance.getEntriesByType &&
    performance.getEntriesByType("navigation")[0];
  if (nav) {
    vitals.ttfb = Math.round(nav.responseStart);
    if (nav.domContentLoadedEventEnd) vitals.dcl = Math.round(nav.domContentLoadedEventEnd);
  }

  // No-op when the author opted out, so an opted-out app does not pay for
  // observers whose output would be discarded at emit time anyway.
  function observe(type, handler, opts) {
    if (!analyticsEnabled) return null;
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
    if (!analyticsEnabled) return;
    if (vitalsSent) return;
    vitalsSent = true;
    if (Object.keys(vitals).length) push("oxy-web-vitals", vitals);
  }

  // ── errors ────────────────────────────────────────────────────────────────
  //
  // Counts by name AND, since 2026-09, the message and stack. The old rule was
  // "never messages or stacks", because a thrown value routinely carries a row
  // id, a SQL fragment, or a user's own input. That rule also made a
  // white-screened app report `{TypeError: 3}` and nothing anyone could act on,
  // so it was traded deliberately (design doc §5.1): detail goes to a SEPARATE
  // server-side table with 30-day retention behind the app-admin gate, while
  // the counts keep feeding the 90-day Activity rollup as before. One event,
  // two sinks.
  //
  // Three bounds, because an error channel is the one place a page can shout:
  //
  //  1. **Dedup per session by stack.** A fault in a render loop fires
  //     thousands of times; it is one problem. The first occurrence of each
  //     distinct (name, stack) carries detail, the rest only increment counts.
  //  2. **A cap per flush**, independent of the batch cap, so a page throwing
  //     five different errors cannot spend the whole batch on itself and starve
  //     `app-ready` or web vitals.
  //  3. **Truncation** of both message and stack, here rather than only on the
  //     server, so the bytes are never put on the wire in the first place.
  var errors = {};
  var errorDetails = [];
  var seenStacks = {};
  var MAX_ERROR_DETAILS = 5;
  var MAX_MESSAGE_CHARS = 1000;
  var MAX_STACK_CHARS = 8000;

  /** Origin-stripped so the same fault hashes identically on the subpath and
   *  subdomain surfaces — otherwise one bug groups as two. */
  function normaliseStack(stack) {
    return String(stack || "").replace(/https?:\/\/[^/\s)]+/g, "");
  }

  /** djb2, base36. Not a cryptographic hash and does not need to be — it is a
   *  grouping key, and a collision merges two error groups in a debugging UI
   *  rather than losing data. */
  function hashString(input) {
    var h = 5381;
    for (var i = 0; i < input.length; i++) {
      h = ((h << 5) + h + input.charCodeAt(i)) | 0;
    }
    return (h >>> 0).toString(36);
  }

  function noteError(name, message, stack, kind, traceId) {
    var key = String(name || "Error").slice(0, 40);
    errors[key] = (errors[key] || 0) + 1;

    // Counts are a health signal and ride past the opt-out. The DETAIL —
    // message, stack, screen path — does not: it is free text from the page,
    // which is exactly what the exemption's argument says health signals never
    // carry. An app with `analytics: false` still reports THAT it threw, so
    // availability is unaffected; it just stops shipping what the text said.
    if (!analyticsEnabled) return;

    var normalised = normaliseStack(stack);
    var hash = hashString(key + "|" + normalised);
    // Already reported this fault this session, or the flush is full: the
    // count above still moves, so nothing is lost that the rollup measures.
    if (seenStacks[hash] || errorDetails.length >= MAX_ERROR_DETAILS) return;
    seenStacks[hash] = 1;
    var detail = {
      n: key,
      m: String(message == null ? "" : message).slice(0, MAX_MESSAGE_CHARS),
      s: String(stack || "").slice(0, MAX_STACK_CHARS),
      h: hash,
      k: kind,
      p: lastPath
    };
    // The SDK stamps `traceId` on an invoke that failed, so an uncaught
    // rejection from `useFunction` names the server-side trace it came from.
    if (typeof traceId === "string" && /^[0-9a-f]{32}$/.test(traceId)) detail.t = traceId;
    errorDetails.push(detail);
  }

  window.addEventListener("error", function (e) {
    var err = e && e.error;
    noteError(
      err && err.name,
      (err && err.message) || (e && e.message),
      err && err.stack,
      "error",
      err && err.traceId
    );
  });
  window.addEventListener("unhandledrejection", function (e) {
    var r = e && e.reason;
    // A rejection can carry anything — an Error, a string, a response object.
    // Only an Error has `name`/`stack`; for the rest the type is the most
    // honest name available, and `String(r)` is the only message there is.
    noteError(
      r && r.name ? r.name : typeof r,
      r && r.message ? r.message : safeString(r),
      r && r.stack,
      "unhandledrejection",
      r && r.traceId
    );
  });

  /** `String(x)` throws on a proxy with a hostile `toString`, and an analytics
   *  path must never be the thing that breaks a page. */
  function safeString(value) {
    try {
      return String(value);
    } catch (e) {
      return "";
    }
  }

  function emitErrors() {
    var names = Object.keys(errors);
    if (!names.length) return;
    // `b` is the build that SERVED this document (`buildId` on the runtime
    // config), not the build that happens to be published when the beacon
    // lands — by then the app may have been re-published, and a stack
    // attributed to the wrong build cannot be resolved against a source map.
    // The client is the only party that knows which build it is running.
    push("oxy-error", {
      counts: errors,
      // The screen path is product analytics wherever it appears — the header
      // above says so about `oxy-pageview`, and a crashing SPA's error paths
      // are a subset of that same list. Gating `noteError`'s detail but not
      // this left the exempt event still shipping a path off-page once per
      // error batch. Availability reads the event's presence and `outcome`,
      // never `route`, so blanking it costs the health signal nothing.
      path: analyticsEnabled ? lastPath : "",
      build: app.buildId || "",
      details: errorDetails
    });
    errors = {};
    errorDetails = [];
    // `seenStacks` is deliberately NOT cleared: dedup is per session, not per
    // flush. Clearing it would re-report the same fault every 15 seconds for as
    // long as the page stays open, which is the behaviour the cap exists to
    // prevent.
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
