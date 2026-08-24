// Oxy splices window.__OXY_APP__ in before </head> at serve time
// (crates/app/src/server/api/customer_apps_serve.rs::inject_app_config),
// so it is defined by the time this script runs.
const app = window.__OXY_APP__;

// `local` is the DuckDB database in examples/config.yml, and `oxymart` is
// its demo sales table — the same one the example dashboards read. No
// credentials, so this works offline on a fresh clone.
const DATABASE = "local";

const KPI_SQL = `SELECT COUNT(DISTINCT Store)              AS stores,
 ROUND(SUM(Weekly_Sales) / 1e6, 1) AS total_sales_musd,
 ROUND(AVG(Weekly_Sales))          AS avg_weekly_sales
FROM oxymart`;

const TOP_STORES_SQL = `SELECT Store,
 ROUND(SUM(Weekly_Sales) / 1e6, 2) AS sales_musd
FROM oxymart
GROUP BY Store
ORDER BY sales_musd DESC
LIMIT 5`;

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

function withSource(node, sql) {
  node.append(text("pre", sql, "source"));
  return node;
}

function errorPanel(title, ...paragraphs) {
  const box = panel(text("div", title, "title"));
  box.classList.add("error");
  for (const p of paragraphs) box.append(text("p", p));
  return box;
}

function cell(value) {
  // textContent, never innerHTML — warehouse values are untrusted.
  if (value === null || value === undefined) return text("td", "null", "null");
  if (typeof value === "object") return text("td", JSON.stringify(value));
  return text("td", String(value), typeof value === "number" ? "num" : undefined);
}

// Every data-plane call rides the same session cookie that authorized the
// page, so there is no token handling anywhere in this bundle.
//
// Failures carry their status, because the caller's advice depends on it:
// the gate chain answers 401 for an expired session and 403 for a
// disallowed origin long before a 404 is possible. It replies with a
// `{ message, code }` envelope, so prefer `message` over the raw body —
// otherwise a panel meant to explain shows JSON.
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

function queryFailed(err) {
  return errorPanel(
    "Query failed",
    err.message,
    `Oxy resolves "${DATABASE}" from the project's config.yml. If the workspace has not been compiled yet, run \`oxy compile\` and reload.`,
  );
}

const KPI_LABELS = {
  stores: "Stores",
  total_sales_musd: "Total sales ($M)",
  avg_weekly_sales: "Avg weekly sales ($)",
};

async function renderKpis() {
  try {
    const { columns, rows } = await query(KPI_SQL);
    if (!rows.length) {
      el("kpis").replaceChildren(
        withSource(panel(text("div", "oxymart is empty.", "state")), KPI_SQL),
      );
      return;
    }
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
    el("kpis").replaceChildren(withSource(panel(grid), KPI_SQL));
  } catch (err) {
    el("kpis").replaceChildren(withSource(queryFailed(err), KPI_SQL));
  }
}

async function renderTopStores() {
  try {
    const { columns, rows, truncated } = await query(TOP_STORES_SQL);
    if (!rows.length) {
      el("top-stores").replaceChildren(
        withSource(panel(text("div", "No rows.", "state")), TOP_STORES_SQL),
      );
      return;
    }
    const table = document.createElement("table");
    const headRow = document.createElement("tr");
    for (const c of columns) headRow.append(text("th", c));
    table.createTHead().append(headRow);

    const body = table.createTBody();
    for (const row of rows) {
      const tr = document.createElement("tr");
      for (const value of row) tr.append(cell(value));
      body.append(tr);
    }

    const scroll = document.createElement("div");
    scroll.className = "scroll";
    scroll.append(table);
    const out = panel(scroll);
    if (truncated) out.append(text("div", "Result truncated at the row cap.", "state"));
    el("top-stores").replaceChildren(withSource(out, TOP_STORES_SQL));
  } catch (err) {
    el("top-stores").replaceChildren(withSource(queryFailed(err), TOP_STORES_SQL));
  }
}

// Display identity for the current viewer. Same session cookie as the
// queries above — the bundle never handles a token.
function shellContext() {
  return fetchJson(`/api/projects/${app.projectId}/shell-context`);
}

// Why this endpoint failed, in the operator's terms. The version hint is
// the LAST guess, not the first: an expired cookie (401) is the common
// failure and a disallowed origin (403) the next, both of which the gate
// chain returns before a 404 is reachable at all. Answering "your server
// is old" to an expired session sends the reader to rebuild a binary.
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

async function renderViewer() {
  try {
    const ctx = await shellContext();
    const dl = document.createElement("dl");
    dl.className = "contract";
    // The SDK types `user` as nullable, so handle it. Today this server
    // always sends one — an unauthenticated caller is turned away by the
    // gate before the handler runs, and lands in the catch below, not
    // here. This branch is forward-compat against the published contract
    // rather than a state you can currently reproduce.
    const rows = ctx.user
      ? [
          ["Name", ctx.user.name],
          ["Email", ctx.user.email],
          ["Organization", ctx.org?.name],
          ["Workspace", ctx.workspace?.name],
        ]
      : [["Viewer", "no session — the server reports no signed-in user"]];
    for (const [label, value] of rows) {
      dl.append(text("dt", label), text("dd", value ?? "—"));
    }
    el("viewer").replaceChildren(panel(dl));
  } catch (err) {
    el("viewer").replaceChildren(
      errorPanel("Could not read the viewer", err.message, viewerHint(err.status)),
    );
  }
}

function renderContract() {
  const dl = el("contract");
  for (const [label, value] of [
    ["Organization", app.orgSlug],
    ["Project", app.projectId],
    ["App", `${app.slug} · ${app.appId}`],
    ["Branch", app.branch],
  ]) {
    dl.append(text("dt", label), text("dd", value ?? "—"));
  }
}

function notServedByOxy() {
  const message = errorPanel(
    "Not served by Oxy",
    "This page is open directly from disk, so it has no runtime identity and no session to query with.",
    "Run `oxy seed`, then open it at /customer-apps/<org>/oxy-starter/.",
  );
  el("contract").replaceWith(message);
  for (const id of ["kpis", "top-stores", "viewer"]) {
    el(id).replaceChildren(panel(text("div", "Nothing loaded — see below.", "state")));
  }
}

if (app) {
  renderContract();
  // Independent panels, so one failing query doesn't blank the other.
  renderKpis();
  renderTopStores();
  renderViewer();
} else {
  notServedByOxy();
}
