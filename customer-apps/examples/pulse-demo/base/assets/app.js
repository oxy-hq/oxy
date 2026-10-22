// Pulse — the demo fixture's page. No build step, no dependencies.
//
// Every panel calls an Oxy Function the way the SDK's `useFunction` does: a
// POST to `<apiBase>/customer-apps/<org>/<app>/fn/<name>` with a fresh
// `traceparent`, answered as server-sent events. Each response carries
// `x-oxy-invocation-id`, the id the console's function views join on.
//
// Two screens, so per-screen Web Vitals have something to split on:
//   /               the leaderboard (`stores`), plus "Send digest" when the
//                   release ships `digest`
//   /store?id=<n>   one store (`store`), plus "Live check". The id is in the
//                   query string, which the runtime never records, so every
//                   store is the one screen `/store`.
//
// The seed stamps two <meta> tags into index.html: the release label and the
// functions this release declares. A page published by hand has neither, and
// then shows every button.

const app = window.__OXY_APP__ || {};
const base = app.basePath || "/";

function meta(name) {
  const node = document.querySelector(`meta[name="${name}"]`);
  return node ? node.getAttribute("content") : null;
}

const RELEASE = meta("pulse-release");
const DECLARED = (meta("pulse-functions") || "").split(",").filter(Boolean);
const declares = (fn) => DECLARED.length === 0 || DECLARED.includes(fn);

// ── DOM helpers: textContent, never innerHTML — function results are data. ──

function text(tag, value, className) {
  const node = document.createElement(tag);
  node.textContent = value;
  if (className) node.className = className;
  return node;
}

function panel(...children) {
  const div = document.createElement("div");
  div.className = "panel";
  div.append(...children);
  return div;
}

function section(title, ...children) {
  const wrap = document.createElement("section");
  wrap.append(text("h2", title), ...children);
  return wrap;
}

function button(label, testid, onClick) {
  const b = text("button", label, "action");
  b.type = "button";
  b.dataset.testid = testid;
  b.addEventListener("click", onClick);
  return b;
}

function link(label, href) {
  const a = text("a", label);
  a.href = href;
  a.dataset.route = "";
  return a;
}

function table(columns, rows) {
  const node = document.createElement("table");
  const head = document.createElement("tr");
  for (const c of columns) head.append(text("th", c.label));
  node.createTHead().append(head);
  const body = node.createTBody();
  for (const row of rows) {
    const tr = document.createElement("tr");
    for (const c of columns) {
      const td = document.createElement("td");
      const value = c.render ? c.render(row) : row[c.key];
      if (value instanceof Node) td.append(value);
      else td.textContent = value === null || value === undefined ? "—" : String(value);
      tr.append(td);
    }
    body.append(tr);
  }
  return node;
}

// ── Calling a function ──────────────────────────────────────────────────────

function traceparent() {
  const hex = (n) =>
    Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, "0"))
      .join("");
  return `00-${hex(16)}-${hex(8)}-01`;
}

/** The SSE frames of one call, folded into `{ status, value, error }`. */
function readFrames(raw) {
  const out = { status: 200, value: null, error: null };
  for (const frame of raw.split("\n\n")) {
    let event = "message";
    let data = "";
    for (const line of frame.split("\n")) {
      if (line.startsWith("event:")) event = line.slice(6).trim();
      else if (line.startsWith("data:")) data += line.slice(5).trim();
    }
    if (!data) continue;
    if (event === "data") out.value = JSON.parse(data);
    if (event === "done") out.status = JSON.parse(data).status ?? 200;
    if (event === "error") out.error = JSON.parse(data).message || "function invocation failed";
  }
  return out;
}

async function call(name, body) {
  const org = encodeURIComponent(app.orgSlug || "");
  const slug = encodeURIComponent(app.slug || "");
  const url = `${app.apiBaseUrl || ""}/customer-apps/${org}/${slug}/fn/${name}`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "text/event-stream",
      traceparent: traceparent()
    },
    body: JSON.stringify(body ?? {})
  });
  const invocation = response.headers.get("x-oxy-invocation-id");
  const raw = await response.text();
  if (!response.ok) return { status: response.status, error: raw.trim(), invocation };
  return { ...readFrames(raw), invocation };
}

/** One call rendered into `slot`: a loading line, then `render(value)`, or the
 *  failure with its invocation id so it can be found in the console. */
async function fill(slot, name, body, render) {
  slot.replaceChildren(panel(text("div", `Calling ${name}…`, "state")));
  let result;
  try {
    result = await call(name, body);
  } catch (err) {
    slot.replaceChildren(panel(text("div", `${name}: ${err.message}`, "state error")));
    return;
  }
  const id = result.invocation ? `invocation ${result.invocation}` : "no invocation id";
  const footer = text("div", `${name} · ${result.status} · ${id}`, "footer");
  if (result.error || result.status >= 400) {
    const message = result.error || result.value?.error || `answered ${result.status}`;
    slot.replaceChildren(panel(text("div", message, "state error"), footer));
    return;
  }
  slot.replaceChildren(panel(render(result.value), footer));
}

// ── Screens ─────────────────────────────────────────────────────────────────

function route() {
  const path = location.pathname.startsWith(base) ? location.pathname.slice(base.length) : "";
  const screen = path.replace(/^\/+|\/+$/g, "");
  return { screen: screen === "store" ? "store" : "home", params: new URLSearchParams(location.search) };
}

function storeHref(id) {
  return `${base}store?${new URLSearchParams({ id: String(id) })}`;
}

function homeScreen() {
  const board = document.createElement("div");
  fill(board, "stores", {}, (value) =>
    table(
      [
        { label: "Store", render: (r) => link(`Store ${r.store}`, storeHref(r.store)) },
        { label: "Sales ($M)", key: "sales_musd" }
      ],
      value.stores || []
    )
  );
  const screens = [section("Top stores by sales", board)];
  if (declares("digest")) {
    const out = document.createElement("div");
    const send = button("Send digest (opens an email preview)", "pulse-send-digest", () =>
      fill(out, "digest", { send: true }, (v) => text("pre", (v.lines || []).join("\n"), "source"))
    );
    screens.push(section("Weekly digest", send, out));
  }
  return screens;
}

function storeScreen(params) {
  const raw = params.get("id") || "";
  // `/^\d+$/`, not parseInt: "7abc" must not render store 7.
  if (!/^\d+$/.test(raw)) {
    return [section("Store", panel(text("div", `"${raw}" is not a store number.`, "state error")))];
  }
  const id = Number(raw);
  const detail = document.createElement("div");
  const render = (value) => {
    const box = document.createElement("div");
    const s = value.store || {};
    box.append(
      table(
        [
          { label: "Store", key: "store" },
          { label: "Weeks", key: "weeks" },
          { label: "Sales ($M)", key: "sales_musd" },
          { label: "Avg weekly ($)", key: "avg_weekly_sales" }
        ],
        [s]
      )
    );
    if (value.rank) box.append(text("p", `Rank ${value.rank} in the latest snapshot.`, "note"));
    if (value.live) box.append(text("pre", JSON.stringify(value.live, null, 2), "source"));
    return box;
  };
  fill(detail, "store", { store: id }, render);
  const live = button("Live check", "pulse-live-check", () =>
    fill(detail, "store", { store: id, live: true }, render)
  );
  const back = link("← All stores", base);
  return [section(`Store ${id}`, back, detail, live)];
}

function render() {
  const { screen, params } = route();
  const nav = document.getElementById("nav");
  nav.replaceChildren(link("Leaderboard", base), link("Store 7", storeHref(7)));
  const body = screen === "store" ? storeScreen(params) : homeScreen();
  document.getElementById("screen").replaceChildren(...body);
}

document.addEventListener("click", (e) => {
  if (e.defaultPrevented || e.button !== 0) return;
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const a = e.target.closest("a[data-route]");
  if (!a) return;
  e.preventDefault();
  history.pushState(null, "", a.getAttribute("href"));
  render();
});
window.addEventListener("popstate", render);

document.getElementById("release").textContent = RELEASE
  ? `Release ${RELEASE} · build ${app.buildId || "unknown"} · ${app.environment || "production"}`
  : `Build ${app.buildId || "unknown"}`;
render();
