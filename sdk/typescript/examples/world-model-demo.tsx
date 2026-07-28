// Example app: the World Model **node interface** (`useWorldModel`).
//
// The whole interaction model is: render a node, click a verb, get more nodes
// back, recurse. `useWorldModel()` returns the interface scoped to the app's
// project; `world.metric(id)` is the live handle you carry around, and every
// button below is one of its verbs (`expand` / `drivers` / `explain` / `size`
// / `drill`).
//
// ── How to run ──────────────────────────────────────────────────────────────
// Like `world-model-analysis.tsx`, this is illustrative component code — it
// needs a customer-app dev harness (Vite + `@oxy-hq/vite-plugin`, which
// proxies `/api` to a running Oxy backend):
//
//   1. Run Oxy locally with a project whose semantic layer declares the
//      measures you point `root` at:  `oxy serve` (default :3000).
//   2. Scaffold a bundle:  `pnpm dlx create-oxy-app my-app`  (the `vite`
//      template is pre-wired with the plugin + an `oxy-app.json`).
//   3. Copy this file into the scaffold's `src/App.tsx`.
//   4. `pnpm install && pnpm dev`  → http://localhost:5173.
//
// The measure ids and periods below are placeholders — point `root` at a
// measure your project actually declares.

import {
  type ExpandedNode,
  type ExplainResult,
  type OpportunityResult,
  OxyAppProvider,
  type SensitivityResult,
  useWorldModel,
  WorldModelScopeUnsupportedError
} from "@oxy-hq/sdk";
import type { CSSProperties, ReactNode } from "react";
import { useCallback, useMemo, useRef, useState } from "react";

export default function App() {
  return (
    <OxyAppProvider fallback={<p style={{ padding: 24 }}>Loading…</p>}>
      <WorldModelExplorer root='orders.net_revenue' timeDimension='orders.order_date' />
    </OxyAppProvider>
  );
}

type Verb = "expand" | "drivers" | "explain" | "size";

/** A breadcrumb entry — `k` is a stable React key (the same metric id can
 *  appear more than once in a navigation path). */
type Crumb = { id: string; k: number };

const CURRENT: [string, string] = ["2026-06-01", "2026-06-30"];
const PREVIOUS: [string, string] = ["2026-05-01", "2026-05-31"];

function WorldModelExplorer({ root, timeDimension }: { root: string; timeDimension: string }) {
  const world = useWorldModel();

  const [trail, setTrail] = useState<Crumb[]>([{ id: root, k: 0 }]);
  const nextKey = useRef(1);
  const [scope, setScope] = useState<Record<string, string>>({});
  const [active, setActive] = useState<Verb | null>(null);
  const [result, setResult] = useState<{ kind: Verb; data: unknown } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [alpha, setAlpha] = useState<string | null>(null);
  const runToken = useRef(0);

  const focusId = trail[trail.length - 1].id;
  const scoped = Object.keys(scope).length > 0;

  // Build the handle fresh from the focused id + accumulated drill scope.
  const handle = useMemo(() => {
    const h = world.metric(focusId);
    return scoped ? h.drill(scope) : h;
  }, [world, focusId, scope, scoped]);

  const reset = useCallback(() => {
    runToken.current++;
    setActive(null);
    setResult(null);
    setError(null);
    setAlpha(null);
    setLoading(false);
  }, []);

  const run = useCallback((verb: Verb, fn: () => Promise<unknown>) => {
    const token = ++runToken.current;
    setActive(verb);
    setResult(null);
    setError(null);
    setAlpha(null);
    setLoading(true);
    // `fn()` may throw synchronously (a drilled node rejects value verbs);
    // Promise.resolve().then(fn) funnels both sync throws and rejections here.
    Promise.resolve()
      .then(fn)
      .then((data) => {
        if (runToken.current !== token) return;
        setResult({ kind: verb, data });
        setLoading(false);
      })
      .catch((e: unknown) => {
        if (runToken.current !== token) return;
        setLoading(false);
        if (e instanceof WorldModelScopeUnsupportedError) setAlpha(e.message);
        else setError(e instanceof Error ? e.message : String(e));
      });
  }, []);

  const focusChild = (id: string) => {
    setTrail((t) => [...t, { id, k: nextKey.current++ }]);
    reset();
  };
  const jumpTo = (i: number) => {
    setTrail((t) => t.slice(0, i + 1));
    reset();
  };

  const verbs: { key: Verb; label: string; go: () => void }[] = [
    { key: "expand", label: "Expand", go: () => run("expand", () => handle.expand()) },
    { key: "drivers", label: "Drivers", go: () => run("drivers", () => handle.drivers()) },
    {
      key: "explain",
      label: "Explain",
      go: () =>
        run("explain", () =>
          handle.explain({
            time_dimension: timeDimension,
            current_period: CURRENT,
            previous_period: PREVIOUS
          })
        )
    },
    {
      key: "size",
      label: "Size",
      go: () => run("size", () => handle.size({ time_dimension: timeDimension, period: CURRENT }))
    }
  ];

  return (
    <main style={S.app}>
      <header>
        <h1 style={S.h1}>World Model explorer</h1>
        <p style={S.sub}>
          Walk your business from one top-line metric — every node is a live handle.
        </p>
      </header>

      <nav style={S.crumbs} aria-label='trail'>
        {trail.map((c, i) =>
          i === trail.length - 1 ? (
            <span key={c.k} style={S.crumbCur}>
              {c.id}
            </span>
          ) : (
            <span key={c.k}>
              <button type='button' style={S.crumbBtn} onClick={() => jumpTo(i)}>
                {c.id}
              </button>
              <span style={S.faint}> / </span>
            </span>
          )
        )}
      </nav>

      <section style={S.panel}>
        <div style={S.nodeId}>{focusId}</div>
        <div style={S.sub}>{scoped ? "drilled" : "population"}</div>

        <div style={S.verbs}>
          {verbs.map((v) => (
            <button
              type='button'
              key={v.key}
              style={{ ...S.verb, ...(active === v.key ? S.verbActive : null) }}
              disabled={loading}
              onClick={v.go}
            >
              {v.label}
            </button>
          ))}
        </div>

        <DrillBar scope={scope} setScope={setScope} onChange={reset} />

        <div style={S.result}>
          {loading ? <p style={S.faint}>Running {active}…</p> : null}
          {error ? <div style={S.err}>{error}</div> : null}
          {alpha ? (
            <div style={S.alpha}>
              <strong>Alpha:</strong> {alpha}
            </div>
          ) : null}
          {!loading && !error && !alpha && result ? (
            <ResultView result={result} onFocus={focusChild} />
          ) : null}
          {!loading && !error && !alpha && !result ? (
            <p style={S.faint}>Pick a verb to explore this node.</p>
          ) : null}
        </div>
      </section>
    </main>
  );
}

// ── Verb result renderers ─────────────────────────────────────────────────────

function ResultView({
  result,
  onFocus
}: {
  result: { kind: Verb; data: unknown };
  onFocus: (id: string) => void;
}) {
  switch (result.kind) {
    case "expand":
      return <ExpandView nodes={result.data as ExpandedNode[]} onFocus={onFocus} />;
    case "drivers":
      return <DriversView data={result.data as SensitivityResult} />;
    case "explain":
      return <ExplainView data={result.data as ExplainResult} />;
    case "size":
      return <SizeView data={result.data as OpportunityResult} />;
  }
}

/** `expand` — one hop of children (components + drivers) as clickable cards. */
function ExpandView({ nodes, onFocus }: { nodes: ExpandedNode[]; onFocus: (id: string) => void }) {
  if (nodes.length === 0) return <p style={S.faint}>Leaf measure — no components or drivers.</p>;
  return (
    <div style={S.cards}>
      {nodes.map((c) => (
        <button type='button' key={c.node.id} style={S.card} onClick={() => onFocus(c.node.id)}>
          <div style={{ fontWeight: 600 }}>{c.node.label}</div>
          <div style={S.mid}>{c.node.id}</div>
          <span style={{ ...S.tag, ...(c.edge.kind === "driver" ? S.tagDriver : S.tagComponent) }}>
            {c.edge.kind}
            {c.edge.kind === "driver" ? ` · ${c.edge.direction} · ${c.edge.strength}` : ""}
          </span>
        </button>
      ))}
    </div>
  );
}

/** `drivers` — declared drivers ranked by influence (sensitivity). */
function DriversView({ data }: { data: SensitivityResult }) {
  if (data.drivers.length === 0)
    return <p style={S.faint}>No declared drivers for {data.target}.</p>;
  return (
    <table style={S.table}>
      <thead>
        <tr>
          <Th>Driver</Th>
          <Th>Direction</Th>
          <Th>Strength</Th>
          <Th>Form</Th>
        </tr>
      </thead>
      <tbody>
        {data.drivers.map((d) => (
          <tr key={`${d.measure}>${d.path.join(">")}`}>
            <Td mono>{d.measure}</Td>
            <Td>{d.direction}</Td>
            <Td>{d.strength}</Td>
            <Td>{d.form ?? "—"}</Td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** `explain` — period-over-period root cause: top contributors + coverage. */
function ExplainView({ data }: { data: ExplainResult }) {
  return (
    <div>
      <p style={S.meta}>
        Δ {signed(data.target_delta)} on {data.target} — {(data.coverage * 100).toFixed(0)}%
        explained ({fmt(data.target_previous)} → {fmt(data.target_current)})
      </p>
      <table style={S.table}>
        <thead>
          <tr>
            <Th>Contributor</Th>
            <Th>Δ</Th>
            <Th>Share</Th>
          </tr>
        </thead>
        <tbody>
          {data.nodes.slice(0, 8).map((n) => (
            <tr key={`${n.measure}:${n.root_fraction}:${n.delta}`}>
              <Td mono>{n.measure}</Td>
              <Td num>{signed(n.delta)}</Td>
              <Td num>{(n.root_fraction * 100).toFixed(0)}%</Td>
            </tr>
          ))}
        </tbody>
      </table>
      {data.warnings && data.warnings.length > 0 ? (
        <p style={S.meta}>⚠ {data.warnings.map((w) => w.type).join(", ")}</p>
      ) : null}
    </div>
  );
}

/** `size` — addressable upside per dimension (match-the-best). */
function SizeView({ data }: { data: OpportunityResult }) {
  if (data.dimensions.length === 0)
    return <p style={S.faint}>No sizeable gaps for {data.target}.</p>;
  return (
    <div>
      <p style={S.meta}>Addressable upside on {data.target} — top segments per dimension.</p>
      {data.dimensions.map((dim) => (
        <div key={dim.dimension} style={{ marginBottom: 14 }}>
          <strong>{dim.dimension}</strong> — total upside {fmt(dim.total_upside)}{" "}
          <span style={S.faint}>(vs {dim.benchmark_basis})</span>
          <table style={S.table}>
            <thead>
              <tr>
                <Th>Segment</Th>
                <Th>Current</Th>
                <Th>Gap</Th>
                <Th>Upside</Th>
              </tr>
            </thead>
            <tbody>
              {dim.segments.slice(0, 5).map((s) => (
                <tr key={s.segment}>
                  <Td mono>{s.segment}</Td>
                  <Td num>{fmt(s.current_value)}</Td>
                  <Td num>{fmt(s.gap)}</Td>
                  <Td num>{fmt(s.upside)}</Td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
}

/** The drill scope editor — add `dimension = value` chips. Value verbs on a
 *  drilled node surface the alpha `WorldModelScopeUnsupportedError`. */
function DrillBar({
  scope,
  setScope,
  onChange
}: {
  scope: Record<string, string>;
  setScope: (next: Record<string, string>) => void;
  onChange: () => void;
}) {
  const [dim, setDim] = useState("");
  const [val, setVal] = useState("");
  const add = () => {
    if (!dim.trim() || !val.trim()) return;
    setScope({ ...scope, [dim.trim()]: val.trim() });
    setDim("");
    setVal("");
    onChange();
  };
  const remove = (key: string) => {
    const next = { ...scope };
    delete next[key];
    setScope(next);
    onChange();
  };
  return (
    <div style={S.drill}>
      <span style={S.faint}>Drill:</span>
      {Object.entries(scope).map(([k, v]) => (
        <span key={k} style={S.chip}>
          {k}={v}
          <button
            type='button'
            style={S.chipX}
            onClick={() => remove(k)}
            aria-label={`remove ${k}`}
          >
            ×
          </button>
        </span>
      ))}
      <input
        style={S.input}
        value={dim}
        onChange={(e) => setDim(e.target.value)}
        placeholder='dimension'
      />
      <input
        style={S.input}
        value={val}
        onChange={(e) => setVal(e.target.value)}
        placeholder='value'
        onKeyDown={(e) => e.key === "Enter" && add()}
      />
      <button type='button' style={S.verb} onClick={add}>
        + scope
      </button>
    </div>
  );
}

// ── Small helpers ─────────────────────────────────────────────────────────────

const fmt = (n: number): string => n.toLocaleString(undefined, { maximumFractionDigits: 2 });
const signed = (n: number): string => (n >= 0 ? "+" : "") + fmt(n);

function Th({ children }: { children: ReactNode }) {
  return <th style={S.th}>{children}</th>;
}
function Td({ children, mono, num }: { children: ReactNode; mono?: boolean; num?: boolean }) {
  return (
    <td style={{ ...S.td, ...(mono ? S.tdMono : null), ...(num ? S.tdNum : null) }}>{children}</td>
  );
}

// Inline styles keep this example a single self-contained file.
const S: Record<string, CSSProperties> = {
  app: {
    maxWidth: 900,
    margin: "0 auto",
    padding: "32px 20px 64px",
    fontFamily: "ui-sans-serif, system-ui, sans-serif",
    color: "#101418"
  },
  h1: { fontSize: 24, margin: "0 0 4px", letterSpacing: "-0.01em" },
  sub: { margin: 0, color: "#626c76", fontSize: 13 },
  faint: { color: "#97a1ab", fontSize: 13 },
  crumbs: {
    display: "flex",
    flexWrap: "wrap",
    gap: 4,
    alignItems: "center",
    fontSize: 13,
    margin: "18px 0 10px",
    fontFamily: "ui-monospace, monospace"
  },
  crumbBtn: {
    background: "none",
    border: "none",
    color: "#1f6feb",
    cursor: "pointer",
    font: "inherit",
    padding: 2
  },
  crumbCur: { fontWeight: 600 },
  panel: {
    background: "#fff",
    border: "1px solid #e3e6ea",
    borderRadius: 12,
    padding: "18px 20px"
  },
  nodeId: { fontFamily: "ui-monospace, monospace", fontSize: 17, fontWeight: 600 },
  verbs: { display: "flex", flexWrap: "wrap", gap: 6, margin: "14px 0 4px" },
  verb: {
    font: "inherit",
    fontSize: 13,
    fontWeight: 550,
    padding: "6px 12px",
    borderRadius: 999,
    cursor: "pointer",
    border: "1px solid #e3e6ea",
    background: "#fff",
    color: "#101418"
  },
  verbActive: { background: "#1f6feb", borderColor: "#1f6feb", color: "#fff" },
  drill: { display: "flex", flexWrap: "wrap", gap: 6, alignItems: "center", marginTop: 10 },
  chip: {
    fontFamily: "ui-monospace, monospace",
    fontSize: 12,
    background: "#f3eefe",
    color: "#8250df",
    borderRadius: 999,
    padding: "3px 9px",
    display: "inline-flex",
    gap: 6,
    alignItems: "center"
  },
  chipX: {
    border: "none",
    background: "none",
    color: "#8250df",
    cursor: "pointer",
    fontSize: 14,
    lineHeight: 1,
    padding: 0
  },
  input: {
    font: "inherit",
    fontSize: 13,
    fontFamily: "ui-monospace, monospace",
    padding: "5px 8px",
    border: "1px solid #e3e6ea",
    borderRadius: 8,
    minWidth: 110
  },
  result: { marginTop: 16, borderTop: "1px solid #e3e6ea", paddingTop: 14 },
  err: {
    background: "#fdecec",
    border: "1px solid #f4b6b6",
    color: "#9b1c1c",
    borderRadius: 8,
    padding: "10px 12px",
    fontSize: 13
  },
  alpha: {
    background: "#fff7e6",
    border: "1px solid #f2d492",
    color: "#7a5a12",
    borderRadius: 8,
    padding: "10px 12px",
    fontSize: 13
  },
  cards: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(210px, 1fr))", gap: 8 },
  card: {
    textAlign: "left",
    font: "inherit",
    cursor: "pointer",
    border: "1px solid #e3e6ea",
    borderRadius: 10,
    background: "#fff",
    padding: "10px 12px"
  },
  mid: {
    fontFamily: "ui-monospace, monospace",
    fontSize: 11,
    color: "#97a1ab",
    marginTop: 2,
    wordBreak: "break-all"
  },
  tag: {
    display: "inline-block",
    fontSize: 10.5,
    textTransform: "uppercase",
    letterSpacing: "0.05em",
    padding: "2px 6px",
    borderRadius: 999,
    marginTop: 8
  },
  tagComponent: { background: "#eaf1fe", color: "#1f6feb" },
  tagDriver: { background: "#f3eefe", color: "#8250df" },
  table: { width: "100%", borderCollapse: "collapse", fontSize: 13, marginTop: 6 },
  th: {
    textAlign: "left",
    color: "#97a1ab",
    fontWeight: 600,
    fontSize: 11,
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    padding: "5px 8px"
  },
  td: { padding: "6px 8px", borderTop: "1px solid #e3e6ea" },
  tdMono: { fontFamily: "ui-monospace, monospace", fontSize: 12 },
  tdNum: { textAlign: "right", fontVariantNumeric: "tabular-nums" },
  meta: { color: "#626c76", fontSize: 13, marginBottom: 8 }
};
