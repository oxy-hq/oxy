import { MarkerType, type Edge as RFEdge } from "@xyflow/react";
import type { FieldDiffStatus } from "./types";

// ── Layout constants ──────────────────────────────────────────────────────────

export const NODE_W = 190;

export const HANDLE_STYLE = { opacity: 0, pointerEvents: "none" as const };

// ── Edge helpers ──────────────────────────────────────────────────────────────

export function statusEdgeColor(status?: FieldDiffStatus | "dashed"): string {
  if (status === "modified") return "rgb(245 158 11 / 0.7)";
  if (status === "removed") return "rgb(239 68 68 / 0.7)";
  if (status === "added") return "rgb(16 185 129 / 0.7)";
  return "rgb(113 113 122 / 0.4)";
}

export function makeEdge(
  id: string,
  source: string,
  target: string,
  opts: {
    sourceHandle?: string;
    targetHandle?: string;
    status?: FieldDiffStatus;
    dashed?: boolean;
    straight?: boolean;
  } = {}
): RFEdge {
  const color = opts.dashed ? "rgb(113 113 122 / 0.4)" : statusEdgeColor(opts.status);
  return {
    id,
    source,
    target,
    sourceHandle: opts.sourceHandle,
    targetHandle: opts.targetHandle,
    type: opts.straight ? "straight" : "smoothstep",
    style: {
      stroke: color,
      strokeWidth: 1.5,
      ...(opts.dashed ? { strokeDasharray: "4 4" } : {})
    },
    markerEnd: { type: MarkerType.ArrowClosed, color, width: 16, height: 16 }
  };
}

// ── ELK edge routing ──────────────────────────────────────────────────────────

export interface ElkPoint {
  x: number;
  y: number;
}

export interface ElkSection {
  startPoint: ElkPoint;
  endPoint: ElkPoint;
  bendPoints?: ElkPoint[];
}

/** Converts an ELK edge section (with optional bendpoints) to a smooth SVG cubic-bezier path. */
export function elkSectionToSvgPath(section: ElkSection): string {
  const pts = [section.startPoint, ...(section.bendPoints ?? []), section.endPoint];
  if (pts.length < 2) return "";
  let d = `M ${pts[0].x},${pts[0].y}`;
  for (let i = 1; i < pts.length; i++) {
    const p0 = pts[i - 1];
    const p1 = pts[i];
    const midY = (p0.y + p1.y) / 2;
    d += ` C ${p0.x},${midY} ${p1.x},${midY} ${p1.x},${p1.y}`;
  }
  return d;
}

// ── Line diff computation ─────────────────────────────────────────────────────

export type DiffOp = { type: "add" | "remove" | "keep"; line: string };

const MAX_DIFF_LINES = 300;

export function computeLineDiff(oldContent: string, newContent: string): DiffOp[] {
  const a = oldContent ? oldContent.split("\n") : [];
  const b = newContent ? newContent.split("\n") : [];
  const aS = a.slice(0, MAX_DIFF_LINES);
  const bS = b.slice(0, MAX_DIFF_LINES);
  const m = aS.length;
  const n = bS.length;

  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = 1; i <= m; i++)
    for (let j = 1; j <= n; j++)
      dp[i][j] =
        aS[i - 1] === bS[j - 1] ? dp[i - 1][j - 1] + 1 : Math.max(dp[i - 1][j], dp[i][j - 1]);

  const ops: DiffOp[] = [];
  let i = m;
  let j = n;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && aS[i - 1] === bS[j - 1]) {
      ops.unshift({ type: "keep", line: aS[i - 1] });
      i--;
      j--;
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      ops.unshift({ type: "add", line: bS[j - 1] });
      j--;
    } else {
      ops.unshift({ type: "remove", line: aS[i - 1] });
      i--;
    }
  }
  return ops;
}
