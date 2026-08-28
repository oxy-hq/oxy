// Oxy splices window.__OXY_APP__ in before </head> at serve time
// (crates/app/src/server/api/custom_apps_serve/rewrite.rs::inject_app_config),
// so it is defined by the time this script runs.
const app = window.__OXY_APP__;

// `local` is the DuckDB database in examples/config.yml, and `oxymart` is
// its demo sales table — the same one the example dashboards read. No
// credentials, so this works offline on a fresh clone.
const DATABASE = "local";

// ── Routing ─────────────────────────────────────────────────────────────────
//
// Four screens, addressed by URL, navigated with the History API. Both halves
// of the URL carry state, because both are things a real app needs and they
// behave differently:
//
//   - The **path** picks the screen (`/stores`, `/stores/20`). The platform
//     runtime reports an SPA pageview per path, so these show up individually
//     on the app's Activity tab.
//   - The **query** carries state within a screen (`?limit=10`). Query strings
//     are deliberately never recorded — so this half is linkable but private,
//     which is why it is where an app puts "which month, which vendor".
//
// `basePath` is the prefix this app is mounted under, with a trailing slash,
// and it is the SAME string on the subpath and subdomain surfaces — so
// routing off it needs no per-surface branch.
const base = app.basePath || "/";

const SCREENS = ["overview", "stores", "identity"];

/** The current URL, decoded into a screen name, an optional argument, and the
 *  query params. Anything unrecognised falls back to the overview rather than
 *  rendering an error: a URL is user input, and a 404 screen inside an app is
 *  a dead end where the front page is not. */
function route() {
  const path = location.pathname.startsWith(base)
    ? location.pathname.slice(base.length)
    : "";
  const [name, arg] = path.replace(/^\/+|\/+$/g, "").split("/");
  return {
    screen: SCREENS.includes(name) ? name : "overview",
    arg,
    params: new URLSearchParams(location.search),
  };
}

/** Build an in-app URL. Always absolute-from-base, so it is correct whether the
 *  app is at `/customer-apps/<org>/<app>/` or at the root of its own subdomain.
 *
 *  `segments` are encoded, `params` are encoded by `URLSearchParams`. The store
 *  numbers this builds from are integers out of the warehouse and inert, but a
 *  value read from a query result is still a value you did not choose — the same
 *  stance as `textContent, never innerHTML` below, applied to the other sink. */
function url(path, params, ...segments) {
  const tail = segments.map((s) => `/${encodeURIComponent(s)}`).join("");
  const query = params ? `?${new URLSearchParams(params)}` : "";
  return `${base}${path}${tail}${query}`;
}

// How the current render was reached — the one fact a back/forward test needs
// and the URL cannot tell you. Set by whichever entry point ran.
let lastMove = "load";

/** Navigate without a document load. `pushState` is what puts an entry on the
 *  session history stack, which is what makes Back come back here. */
function go(href) {
  if (href === location.pathname + location.search) return;
  lastMove = "link";
  history.pushState(null, "", href);
  render();
}

// Back and forward. The browser has already changed the URL by the time this
// fires — the handler's whole job is to re-render from it, which is why every
// screen reads its state from the URL rather than from a variable.
window.addEventListener("popstate", () => {
  lastMove = "popstate";
  render();
});

// One delegated listener rather than a handler per link: screens re-render
// constantly and re-binding on every render is how a router leaks.
//
// Modifier and middle clicks are handed back to the browser untouched. They
// mean "not here" — a new tab, a new window — and intercepting them is the
// single most common way an in-app router breaks cmd-click.
document.addEventListener("click", (e) => {
  if (e.defaultPrevented || e.button !== 0) return;
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const link = e.target.closest("a[data-route]");
  if (!link) return;
  e.preventDefault();
  go(link.getAttribute("href"));
});

// ── DOM helpers ─────────────────────────────────────────────────────────────

const el = (id) => document.getElementById(id);

function text(tag, value, className) {
  const node = document.createElement(tag);
  node.textContent = value;
  if (className) node.className = className;
  return node;
}

function panel(...children) {
  const div = document.createElement("div");
  div.className = "panel";
  for (const child of children) div.append(child);
  return div;
}

/** Tag a node for the committed browser flow that drives this app
 *  (`web-app/tests/agentic/flows/customer-apps-oxy-starter-fleet.flow.test.yml`).
 *  A `data-testid` survives copy edits and layout changes, which an id chosen
 *  for CSS does not — and this app has no framework to hang one off. */
function tag(node, testid) {
  node.dataset.testid = testid;
  return node;
}

/** An in-app link. A real `<a href>` with a real destination, so cmd-click,
 *  middle-click, "open in new tab" and "copy link address" all work — the
 *  delegated handler above only intercepts the plain left click. */
function link(label, href, className) {
  const a = text("a", label, className);
  a.href = href;
  a.dataset.route = "";
  return a;
}

function section(title, pattern, ...children) {
  const wrap = document.createElement("section");
  const head = document.createElement("div");
  head.className = "head";
  head.append(text("h2", title), text("span", pattern, "pattern"));
  wrap.append(head, ...children);
  return wrap;
}

function withSource(node, sql) {
  node.append(text("pre", sql, "source"));
  return node;
}

function loading(lines = 2) {
  const box = document.createElement("div");
  box.className = "state";
  for (let i = 0; i < lines; i++) {
    box.append(text("div", "", i === lines - 1 ? "skeleton short" : "skeleton"));
  }
  return panel(box);
}

function cell(value) {
  // textContent, never innerHTML — warehouse values are untrusted.
  if (value === null || value === undefined) return text("td", "null", "null");
  if (typeof value === "object") return text("td", JSON.stringify(value));
  return text("td", String(value), typeof value === "number" ? "num" : undefined);
}

function table(columns, rows, linkFirstCell) {
  const node = document.createElement("table");
  const headRow = document.createElement("tr");
  for (const c of columns) headRow.append(text("th", c));
  node.createTHead().append(headRow);

  const body = node.createTBody();
  for (const row of rows) {
    const tr = document.createElement("tr");
    row.forEach((value, i) => {
      if (i === 0 && linkFirstCell) {
        const td = document.createElement("td");
        td.append(link(String(value), linkFirstCell(value)));
        tr.append(td);
      } else {
        tr.append(cell(value));
      }
    });
    body.append(tr);
  }
  const scroll = document.createElement("div");
  scroll.className = "scroll";
  scroll.append(node);
  return scroll;
}

function errorPanel(title, ...paragraphs) {
  const box = panel(text("div", title, "title"));
  box.classList.add("error");
  for (const p of paragraphs) box.append(text("p", p));
  return box;
}

// ── Data plane ──────────────────────────────────────────────────────────────
//
// Every call rides the same session cookie that authorized the page, so there
// is no token handling anywhere in this bundle.
//
// Failures carry their status, because the caller's advice depends on it: the
// gate chain answers 401 for an expired session and 403 for a disallowed origin
// long before a 404 is possible. It replies with a `{ message, code }` envelope,
// so prefer `message` over the raw body — otherwise a panel meant to explain
// shows JSON.
async function fetchJson(url, init) {
  const response = await fetch(url, init);
  if (!response.ok) {
    const raw = (await response.text()).trim();
    let detail = raw;
    try {
      detail = JSON.parse(raw).message || raw;
    } catch {
      // Not the envelope — a proxy or a static 404 page. Use it verbatim.
    }
    const err = new Error(detail || `${response.status} ${response.statusText}`);
    err.status = response.status;
    throw err;
  }
  return response.json();
}

function query(sql) {
  return fetchJson(`/api/projects/${app.projectId}/query`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ sql, database: DATABASE }),
  });
}

// Why a query failed, in the operator's terms — and the compile hint is the
// LAST guess, not the first. The gate chain answers 401 and 403 before the
// handler that would report a missing table ever runs, so answering "compile
// your workspace" to a rejected origin sends the reader to fix the one thing
// that was never wrong. (It did: a `127.0.0.1` dev URL used to fail the origin
// allowlist, and this panel blamed the workspace.)
function queryFailed(err) {
  if (err.status === 401) {
    return errorPanel(
      "Session expired",
      err.message,
      "The cookie that authorized this page is gone. Reload to sign in again.",
    );
  }
  if (err.status === 403) {
    return errorPanel(
      "Origin refused",
      err.message,
      "The data plane only answers the app's own origin. Open the app from its real " +
        "URL rather than a copy, and in local development reach the dev server the " +
        "same way the backend does.",
    );
  }
  return errorPanel(
    "Query failed",
    err.message,
    `Oxy resolves "${DATABASE}" from the project's config.yml. If the workspace has not been compiled yet, run \`oxy compile\` and reload.`,
  );
}

/** Render `body()` into `slot`, showing a skeleton while it runs and an
 *  explained panel if it throws. Every screen's panels are independent, so one
 *  failing query never blanks the rest of the page. */
async function fill(slot, sql, body) {
  slot.replaceChildren(withSource(loading(), sql));
  try {
    slot.replaceChildren(withSource(await body(), sql));
  } catch (err) {
    slot.replaceChildren(withSource(queryFailed(err), sql));
  }
}

// ── Screen: overview ────────────────────────────────────────────────────────

const KPI_SQL = `SELECT COUNT(DISTINCT Store)              AS stores,
 ROUND(SUM(Weekly_Sales) / 1e6, 1) AS total_sales_musd,
 ROUND(AVG(Weekly_Sales))          AS avg_weekly_sales
FROM oxymart`;

const KPI_LABELS = {
  stores: "Stores",
  total_sales_musd: "Total sales ($M)",
  avg_weekly_sales: "Avg weekly sales ($)",
};

function overviewScreen() {
  const slot = document.createElement("div");
  fill(slot, KPI_SQL, async () => {
    const { columns, rows } = await query(KPI_SQL);
    if (!rows.length) return panel(text("div", "oxymart is empty.", "state"));
    const grid = document.createElement("div");
    grid.className = "kpis";
    columns.forEach((name, i) => {
      const tile = document.createElement("div");
      tile.className = "kpi";
      const raw = rows[0][i];
      tile.append(
        text("div", raw === null ? "—" : Number(raw).toLocaleString(), "value"),
        text("div", KPI_LABELS[name] ?? name, "label"),
      );
      grid.append(tile);
    });
    return tag(panel(grid), "starter-kpis");
  });

  return [
    section("Demo warehouse", "Pattern 1 · scalar aggregate", slot),
    note(
      "One row of KPIs from `POST /api/projects/<projectId>/query`. The screens above " +
        "are real URLs — open one in a new tab, reload it, or walk back through them; " +
        "the app rebuilds itself from the address bar every time.",
    ),
  ];
}

// ── Screen: stores ──────────────────────────────────────────────────────────

const LIMITS = [5, 10, 20];

const topStoresSql = (limit) => `SELECT Store,
 ROUND(SUM(Weekly_Sales) / 1e6, 2) AS sales_musd
FROM oxymart
GROUP BY Store
ORDER BY sales_musd DESC
LIMIT ${limit}`;

function storesScreen({ params }) {
  // The URL is the input, and it is untrusted — a hand-typed `?limit=drop` has
  // to land somewhere sane. Read it, validate against what the screen offers,
  // and fall back; never interpolate what came out of the address bar.
  const requested = Number.parseInt(params.get("limit") ?? "", 10);
  const limit = LIMITS.includes(requested) ? requested : LIMITS[0];
  const sql = topStoresSql(limit);

  const chips = document.createElement("div");
  chips.className = "chips";
  chips.append(text("span", "Show", "chips-label"));
  for (const n of LIMITS) {
    const chip = link(String(n), url("stores", { limit: n }), "chip");
    if (n === limit) chip.classList.add("on");
    chips.append(chip);
  }

  const slot = document.createElement("div");
  fill(slot, sql, async () => {
    const { columns, rows, truncated } = await query(sql);
    if (!rows.length) return panel(text("div", "No rows.", "state"));
    // The first column links onward to that store's screen — the path segment
    // IS the parameter, so the row and its URL are the same fact.
    const out = tag(panel(table(columns, rows, (store) => url("stores", null, store))), "starter-stores");
    if (truncated) out.append(text("div", "Result truncated at the row cap.", "state"));
    return out;
  });

  return [
    section("Top stores by sales", "Pattern 2 · list query", chips, slot),
    note(
      "`?limit=` is state *within* this screen, so it lives in the query string: " +
        "linkable, and back/forward walks the choices you made. A store name links " +
        "onward to `stores/<n>` — a different screen, so it takes a path segment.",
    ),
  ];
}

// ── Screen: one store ───────────────────────────────────────────────────────

const storeSql = (store) => `SELECT CASE WHEN Holiday_Flag = 1 THEN 'Holiday' ELSE 'Regular' END AS week_kind,
 COUNT(*)                          AS weeks,
 ROUND(AVG(Weekly_Sales))          AS avg_weekly_sales,
 ROUND(SUM(Weekly_Sales) / 1e6, 2) AS sales_musd
FROM oxymart
WHERE Store = ${store}
GROUP BY 1
ORDER BY 1`;

function storeScreen(store) {
  const slot = document.createElement("div");
  const sql = storeSql(store);
  fill(slot, sql, async () => {
    const { columns, rows } = await query(sql);
    if (!rows.length) {
      return panel(text("div", `No weeks recorded for store ${store}.`, "state"));
    }
    return tag(panel(table(columns, rows)), "starter-store-detail");
  });

  return [
    section(`Store ${store}`, "Pattern 2b · path parameter", crumb(), slot),
    note(
      "The store number is a **path segment**, and it reaches SQL — so it is parsed " +
        "as an integer and rejected if it is anything else, before it is ever " +
        "interpolated. A URL is user input on every screen, but only here is it " +
        "user input that a database will read.",
    ),
  ];
}

function badStoreScreen(arg) {
  return [
    section(
      "Store",
      "Pattern 2b · path parameter",
      crumb(),
      errorPanel(
        "Not a store number",
        `"${arg}" is not a positive integer, so it was never put into a query.`,
        "Store numbers reach SQL, so they are validated at the edge rather than escaped downstream.",
      ),
    ),
  ];
}

/** The way back up from a store screen. */
function crumb() {
  const box = document.createElement("div");
  box.className = "crumbs";
  box.append(link("← All stores", url("stores")));
  return box;
}

// ── Screen: identity ────────────────────────────────────────────────────────

function contract(rows) {
  const dl = document.createElement("dl");
  dl.className = "contract";
  for (const [label, value] of rows) {
    dl.append(text("dt", label), text("dd", value ?? "—"));
  }
  return dl;
}

// Why shell-context failed, in the operator's terms. The version hint is the
// LAST guess, not the first: an expired cookie (401) is the common failure and
// a disallowed origin (403) the next, both of which the gate chain returns
// before a 404 is reachable at all.
function viewerHint(status) {
  if (status === 401) {
    return "The session cookie expired or was cleared. Reload the page to sign in again.";
  }
  if (status === 403) {
    return "This origin is not allowed to call the data plane. Open the app from its own URL rather than a copy.";
  }
  if (status === 404) {
    return "shell-context is served by oxy from 2026-08 onward; an older server has no such route, and a bundle should render without it.";
  }
  return "The server could not answer. The panels above are independent, so the rest of the page still works.";
}

function identityScreen() {
  const injected = tag(panel(
    contract([
      ["Organization", app.orgSlug],
      ["Project", app.projectId],
      ["App", `${app.slug} · ${app.appId}`],
      ["Branch", app.branch],
      ["Base path", app.basePath],
    ]),
  ), "starter-identity");

  const viewer = document.createElement("div");
  viewer.replaceChildren(loading());
  fetchJson(`/api/projects/${app.projectId}/shell-context`)
    .then((ctx) => {
      // The SDK types `user` as nullable, so handle it. Today this server
      // always sends one — an unauthenticated caller is turned away by the
      // gate before the handler runs, and lands in the catch below, not here.
      const rows = ctx.user
        ? [
            ["Name", ctx.user.name],
            ["Email", ctx.user.email],
            ["Organization", ctx.org?.name],
            ["Workspace", ctx.workspace?.name],
          ]
        : [["Viewer", "no user on the response"]];
      viewer.replaceChildren(tag(panel(contract(rows)), "starter-viewer"));
    })
    .catch((err) => {
      viewer.replaceChildren(
        errorPanel("Viewer unavailable", err.message, viewerHint(err.status)),
      );
    });

  return [
    section("What Oxy injected", "Pattern 3 · runtime identity", injected),
    note(
      "Read from `window.__OXY_APP__`. A bundle never hardcodes its org or project — " +
        "it addresses the data plane with the `projectId` Oxy gives it, and routes " +
        "off the `basePath` Oxy mounts it under.",
    ),
    section("Who is looking at this", "Pattern 4 · viewer identity", viewer),
    note(
      "One request to `/api/projects/<projectId>/shell-context`, on the same session " +
        "cookie that authorized the page. This is **display** identity — a name to " +
        "greet with, an avatar to render. It deliberately carries **no role**: shipping " +
        "one to the browser invites gating on it, and a bundle is just JavaScript the " +
        "viewer can edit. The **verified** half — `appRole`, `orgRole`, `teams`, `kind` " +
        "— lives on `ctx.user` inside an Oxy Function, where the caller cannot reach it.",
    ),
  ];
}

/** A muted paragraph under a section. `**bold**` and `` `code` `` only — enough
 *  to keep the prose readable in the source, and small enough to stay obviously
 *  safe: every span is created with textContent, never innerHTML. */
function note(markup) {
  const p = text("p", "", "note");
  for (const part of markup.split(/(\*\*[^*]+\*\*|`[^`]+`)/g)) {
    if (part.startsWith("**")) p.append(text("b", part.slice(2, -2)));
    else if (part.startsWith("`")) p.append(text("code", part.slice(1, -1)));
    else if (part) p.append(document.createTextNode(part));
  }
  return p;
}

// ── The navigation panel ────────────────────────────────────────────────────
//
// This app exists partly to be *driven* — clicked through, reloaded, walked
// backwards — so it says out loud what the browser just did. `history.length`
// and whether the last render came from a link or from Back are the two facts
// the address bar does not show you.

const moves = [];

function navigationPanel() {
  const rows = contract([
    ["Path", location.pathname],
    ["Query", location.search || "(none)"],
    ["History entries", String(history.length)],
    ["Last move", lastMove],
  ]);

  const log = document.createElement("ol");
  log.className = "movelog";
  for (const move of moves.slice(-8).reverse()) {
    log.append(text("li", move));
  }

  return section(
    "Navigation",
    "Pattern 5 · history",
    tag(panel(rows), "starter-history"),
    log,
    note(
      "Every screen is a real URL reached with `history.pushState`, so Back and " +
        "Forward walk them in order, a reload lands on the screen you were on, and a " +
        "link to one opens where you meant. Nothing here is stored in a variable that " +
        "the URL does not also say.",
    ),
  );
}

// ── Render ──────────────────────────────────────────────────────────────────

function nav(current) {
  const bar = document.createElement("nav");
  for (const [name, label] of [
    ["overview", "Overview"],
    ["stores", "Stores"],
    ["identity", "Identity"],
  ]) {
    const a = link(label, url(name === "overview" ? "" : name));
    tag(a, `starter-nav-${name}`);
    if (name === current) a.className = "on";
    bar.append(a);
  }
  return bar;
}

function render() {
  const { screen, arg, params } = route();

  moves.push(`${lastMove} → ${location.pathname}${location.search}`);

  let body;
  if (screen === "stores" && arg !== undefined) {
    // `/^\d+$/`, not `parseInt` — `parseInt("5abc")` is 5, which would render
    // "Store 5" under a URL that does not say 5. In an app whose whole claim is
    // that the URL is the state, a segment that half-parses has to be refused.
    body = /^\d+$/.test(arg) && Number(arg) > 0 ? storeScreen(Number(arg)) : badStoreScreen(arg);
  } else if (screen === "stores") {
    body = storesScreen({ params });
  } else if (screen === "identity") {
    body = identityScreen();
  } else {
    body = overviewScreen();
  }

  el("nav").replaceChildren(nav(screen));
  el("screen").replaceChildren(...body, navigationPanel());
  // A document load puts the browser at the top; a pushState does not, and
  // arriving halfway down a screen you have not seen reads as a broken page.
  // Back and Forward are left alone — the browser restores their scroll, and
  // overriding that is what makes an app feel like it lost your place.
  if (lastMove === "link") window.scrollTo(0, 0);
}

render();
