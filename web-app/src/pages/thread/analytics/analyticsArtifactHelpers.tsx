import { memo } from "react";
import { DisplayBlock } from "@/components/AppPreview/Displays";
import type { ArtifactItem, SqlItem } from "@/hooks/analyticsSteps";
import type { AnalyticsDisplayBlock } from "@/hooks/useAnalyticsRun";
import type { DataContainer, Display } from "@/types/app";
import type { SqlArtifact } from "@/types/artifact";

// ── JSON helpers ──────────────────────────────────────────────────────────────

/**
 * Parse a tool input/output string that may be double-encoded.
 * analyticsSteps.ts stores toolInput/toolOutput as JSON.stringify(payload),
 * so the value is often a JSON-encoded string that needs a second parse.
 */
export function parseToolJson<T = unknown>(raw: string | undefined): T | null {
  if (!raw) return null;
  try {
    let v = JSON.parse(raw);
    if (typeof v === "string") v = JSON.parse(v);
    return v as T;
  } catch {
    return null;
  }
}

// ── Chart display helpers ─────────────────────────────────────────────────────

export const AGENTIC_DATA_KEY = "__agentic_result__";

export function toDisplayProps(block: AnalyticsDisplayBlock): {
  display: Display;
  data: DataContainer;
} {
  const { config, columns, rows } = block;
  const json = JSON.stringify(
    rows.map((row) => Object.fromEntries(columns.map((col, i) => [col, row[i]]))),
  );
  const data: DataContainer = {
    [AGENTIC_DATA_KEY]: { file_path: AGENTIC_DATA_KEY, json },
  };

  const ct = config.chart_type;
  let display: Display;
  if (ct === "line_chart") {
    display = {
      type: "line_chart",
      x: config.x ?? columns[0] ?? "",
      y: config.y ?? columns[1] ?? "",
      data: AGENTIC_DATA_KEY,
      series: config.series,
      title: config.title,
      xAxisTitle: config.x_axis_label,
      yAxisTitle: config.y_axis_label,
    };
  } else if (ct === "bar_chart") {
    display = {
      type: "bar_chart",
      x: config.x ?? columns[0] ?? "",
      y: config.y ?? columns[1] ?? "",
      data: AGENTIC_DATA_KEY,
      series: config.series,
      title: config.title,
    };
  } else if (ct === "pie_chart") {
    display = {
      type: "pie_chart",
      name: config.name ?? columns[0] ?? "",
      value: config.value ?? columns[1] ?? "",
      data: AGENTIC_DATA_KEY,
      title: config.title,
    };
  } else {
    display = { type: "table", data: AGENTIC_DATA_KEY, title: config.title };
  }

  return { display, data };
}

/** Stable wrapper so parent re-renders don't recreate display/data objects. */
export const AnalyticsDisplayBlockItem = memo(
  ({ block }: { block: AnalyticsDisplayBlock }) => {
    const { display, data } = toDisplayProps(block);
    return <DisplayBlock display={display} data={data} />;
  },
);

// ── SQL artifact helpers ──────────────────────────────────────────────────────

export function sqlArtifactFromSqlItem(item: SqlItem): SqlArtifact {
  return {
    id: item.id,
    name: "SQL Query",
    kind: "execute_sql",
    content: {
      type: "execute_sql",
      value: {
        database: item.database ?? "",
        sql_query: item.sql,
        result: item.result,
        is_result_truncated: false,
      },
    },
  };
}

export function sqlArtifactFromPreviewData(item: ArtifactItem): SqlArtifact | null {
  const input = parseToolJson<Record<string, unknown>>(item.toolInput);
  const table = input?.table;
  if (!table || typeof table !== "string") return null;
  const limit = typeof input?.limit === "number" ? input.limit : 5;
  const sql = `SELECT * FROM "${table}" LIMIT ${limit}`;

  let result: string[][] | undefined;
  const output = parseToolJson<Record<string, unknown>>(item.toolOutput ?? "");
  const cols = output?.columns as string[] | undefined;
  const rows = output?.rows as unknown[] | undefined;
  if (cols?.length) {
    result = [
      cols,
      ...(rows ?? []).map((row) =>
        Array.isArray(row)
          ? row.map((v) => String(v ?? ""))
          : cols.map((col) => String((row as Record<string, unknown>)?.[col] ?? "")),
      ),
    ];
  }

  return {
    id: item.id,
    name: "preview_data",
    kind: "execute_sql",
    content: {
      type: "execute_sql",
      value: { database: "", sql_query: sql, result, is_result_truncated: false },
    },
  };
}

export function sqlArtifactFromExecutePreview(item: ArtifactItem): SqlArtifact | null {
  const input = parseToolJson<Record<string, unknown>>(item.toolInput);
  const sql = input?.sql;
  if (!sql || typeof sql !== "string") return null;

  let result: string[][] | undefined;
  const output = parseToolJson<Record<string, unknown>>(item.toolOutput ?? "");
  const cols = output?.columns as string[] | undefined;
  const rows = output?.rows as unknown[][] | undefined;
  if (cols?.length) {
    result = [
      cols,
      ...(rows ?? []).map((row) =>
        Array.isArray(row)
          ? row.map((v) => String(v ?? ""))
          : cols.map((col) => String((row as Record<string, unknown>)?.[col] ?? "")),
      ),
    ];
  }

  return {
    id: item.id,
    name: "execute_preview",
    kind: "execute_sql",
    content: {
      type: "execute_sql",
      value: { database: "", sql_query: sql, result, is_result_truncated: false },
    },
  };
}
