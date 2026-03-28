import type { ArtifactItem } from "@/hooks/analyticsSteps";
import type { AnalyticsDisplayBlock } from "@/hooks/useAnalyticsRun";
import { AnalyticsDisplayBlockItem, parseToolJson } from "./analyticsArtifactHelpers";

// ── ChartSection ──────────────────────────────────────────────────────────────

export const ChartSection = ({ displayBlocks }: { displayBlocks: AnalyticsDisplayBlock[] }) => {
  if (!displayBlocks.length) return null;
  return (
    <div className='shrink-0 space-y-2 border-t p-4'>
      {displayBlocks.map((block, i) => {
        const key = `${block.config.chart_type}-${block.config.title ?? i}`;
        return <AnalyticsDisplayBlockItem key={key} block={block} />;
      })}
    </div>
  );
};

// ── RawArtifactView ───────────────────────────────────────────────────────────

export const RawArtifactView = ({ item }: { item: ArtifactItem }) => (
  <div className='h-full overflow-auto p-4'>
    <div className='space-y-3'>
      <div>
        <p className='mb-1 font-medium text-muted-foreground text-xs'>Input</p>
        <pre className='whitespace-pre-wrap rounded border bg-muted/50 p-3 font-mono text-[11px]'>
          {item.toolInput}
        </pre>
      </div>
      {item.toolOutput && (
        <div>
          <p className='mb-1 font-medium text-muted-foreground text-xs'>Output</p>
          <pre className='whitespace-pre-wrap rounded border bg-muted/50 p-3 font-mono text-[11px]'>
            {item.toolOutput}
          </pre>
        </div>
      )}
    </div>
  </div>
);

// ── SearchCatalogView ─────────────────────────────────────────────────────────

type CatalogMetric = { name: string; description?: string };
type CatalogDimension = { name: string; description?: string; type?: string };

export const SearchCatalogView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ queries?: string[] }>(item.toolInput);
  const queries = input?.queries ?? [];

  const output = parseToolJson<{ metrics?: CatalogMetric[]; dimensions?: CatalogDimension[] }>(
    item.toolOutput
  );
  const metrics = output?.metrics ?? [];
  const dimensions = output?.dimensions ?? [];

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        {queries.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Search Terms</p>
            <div className='flex flex-wrap gap-1.5'>
              {queries.map((q, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: static list
                <span key={i} className='rounded-full bg-muted px-2.5 py-0.5 text-xs'>
                  {q}
                </span>
              ))}
            </div>
          </div>
        )}

        {metrics.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>
              Metrics ({metrics.length})
            </p>
            <div className='space-y-1'>
              {metrics.map((m, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: static list
                <div key={i} className='rounded border bg-muted/30 px-2.5 py-1.5'>
                  <p className='font-medium text-xs'>{m.name}</p>
                  {m.description && (
                    <p className='text-[11px] text-muted-foreground'>{m.description}</p>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {dimensions.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>
              Dimensions ({dimensions.length})
            </p>
            <div className='space-y-1'>
              {dimensions.map((d, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: static list
                <div
                  key={i}
                  className='flex items-start gap-2 rounded border bg-muted/30 px-2.5 py-1.5'
                >
                  <div className='min-w-0 flex-1'>
                    <p className='font-medium text-xs'>{d.name}</p>
                    {d.description && (
                      <p className='text-[11px] text-muted-foreground'>{d.description}</p>
                    )}
                  </div>
                  {d.type && (
                    <span className='shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground'>
                      {d.type}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {metrics.length === 0 && dimensions.length === 0 && item.toolOutput && (
          <p className='text-muted-foreground text-xs'>No results found.</p>
        )}
      </div>
    </div>
  );
};

// ── SearchProceduresView ──────────────────────────────────────────────────────

type ProcedureRefItem = { name: string; path: string; description?: string };

export const SearchProceduresView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ query?: string }>(item.toolInput);
  const query = input?.query ?? "";

  const output = parseToolJson<{ procedures?: ProcedureRefItem[] }>(item.toolOutput);
  const procedures = output?.procedures ?? [];

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        {query && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Query</p>
            <span className='rounded-full bg-muted px-2.5 py-0.5 text-xs'>{query}</span>
          </div>
        )}

        {procedures.length > 0 ? (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>
              Procedures ({procedures.length})
            </p>
            <div className='space-y-1.5'>
              {procedures.map((p, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: static list
                <div key={i} className='space-y-0.5 rounded border bg-muted/30 px-2.5 py-2'>
                  <p className='font-medium text-xs'>{p.name}</p>
                  <p className='break-all font-mono text-[10px] text-muted-foreground'>{p.path}</p>
                  {p.description && (
                    <p className='text-[11px] text-muted-foreground'>{p.description}</p>
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : (
          item.toolOutput && (
            <p className='text-muted-foreground text-xs'>No matching procedures found.</p>
          )
        )}
      </div>
    </div>
  );
};

// ── SampleColumnView ──────────────────────────────────────────────────────────

export const SampleColumnView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ table?: string; column?: string }>(item.toolInput);
  const table = input?.table ?? "";
  const column = input?.column ?? "";

  const output = parseToolJson<{
    data_type?: string;
    sample_values?: unknown[];
    row_count?: number;
    date_min?: string;
    date_max?: string;
    date_distinct_count?: number;
  }>(item.toolOutput);
  const dataType = output?.data_type ?? "";
  const sampleValues = output?.sample_values ?? [];
  const rowCount = output?.row_count;
  const dateMin = output?.date_min;
  const dateMax = output?.date_max;
  const dateDistinctCount = output?.date_distinct_count;

  const meta = [
    { label: "Table", value: table },
    { label: "Column", value: column },
    { label: "Type", value: dataType || "—" },
    rowCount !== undefined ? { label: "Row Count", value: rowCount.toLocaleString() } : null,
    dateMin ? { label: "Date Min", value: dateMin } : null,
    dateMax ? { label: "Date Max", value: dateMax } : null,
    dateDistinctCount !== undefined
      ? { label: "Distinct Dates", value: dateDistinctCount.toLocaleString() }
      : null
  ].filter(Boolean) as { label: string; value: string }[];

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          {meta.map((m) => (
            <div key={m.label} className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>{m.label}</p>
              <p className='font-medium font-mono text-xs'>{m.value}</p>
            </div>
          ))}
        </div>

        {sampleValues.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>
              Sample Values ({sampleValues.length})
            </p>
            <div className='overflow-hidden rounded border'>
              <table className='w-full text-xs'>
                <thead>
                  <tr className='bg-muted/50'>
                    <th className='px-2.5 py-1.5 text-left font-medium text-muted-foreground'>
                      {column || "value"}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {sampleValues.map((v, i) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: stable ordered list
                    <tr key={i} className={i % 2 === 0 ? "bg-background" : "bg-muted/20"}>
                      <td className='px-2.5 py-1 font-mono'>{String(v ?? "null")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

// ── GetMetricDefinitionView ───────────────────────────────────────────────────

export const GetMetricDefinitionView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ metric?: string }>(item.toolInput);
  const metric = input?.metric ?? item.toolName;

  const output = parseToolJson<{
    formula?: string;
    type?: string;
    table?: string;
    description?: string;
    dimension_count?: number;
  }>(item.toolOutput);

  const fields = [
    { label: "Metric", value: metric },
    { label: "Type", value: output?.type ?? "—" },
    { label: "Table", value: output?.table ?? "—" },
    output?.dimension_count !== undefined
      ? { label: "Dimensions", value: String(output.dimension_count) }
      : null
  ].filter(Boolean) as { label: string; value: string }[];

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          {fields.map((f) => (
            <div key={f.label} className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>{f.label}</p>
              <p className='font-medium font-mono text-xs'>{f.value}</p>
            </div>
          ))}
        </div>
        {output?.description && (
          <div>
            <p className='mb-1 font-medium text-muted-foreground text-xs'>Description</p>
            <p className='text-xs'>{output.description}</p>
          </div>
        )}
        {output?.formula && (
          <div>
            <p className='mb-1 font-medium text-muted-foreground text-xs'>Formula</p>
            <pre className='whitespace-pre-wrap rounded border bg-muted/50 p-2.5 font-mono text-[11px]'>
              {output.formula}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
};

// ── GetJoinPathView ───────────────────────────────────────────────────────────

export const GetJoinPathView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ from_entity?: string; to_entity?: string; from?: string; to?: string }>(item.toolInput);
  // analytics uses from_entity/to_entity; app-builder uses from/to
  const from = input?.from_entity ?? input?.from ?? "?";
  const to = input?.to_entity ?? input?.to ?? "?";

  const output = parseToolJson<{ path?: string; join_type?: string }>(item.toolOutput);

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='flex items-center gap-2 rounded border bg-muted/30 px-3 py-2.5 font-mono text-xs'>
          <span className='font-medium'>{from}</span>
          <span className='text-muted-foreground'>→</span>
          <span className='font-medium'>{to}</span>
          {output?.join_type && (
            <span className='ml-auto shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground'>
              {output.join_type}
            </span>
          )}
        </div>
        {output?.path && (
          <div>
            <p className='mb-1 font-medium text-muted-foreground text-xs'>Join Expression</p>
            <pre className='whitespace-pre-wrap rounded border bg-muted/50 p-2.5 font-mono text-[11px]'>
              {output.path}
            </pre>
          </div>
        )}
        {!output && item.toolOutput && (
          <div>
            <p className='mb-1 font-medium text-muted-foreground text-xs'>Result</p>
            <p className='text-muted-foreground text-xs'>{item.toolOutput}</p>
          </div>
        )}
      </div>
    </div>
  );
};

// ── RenderChartView ───────────────────────────────────────────────────────────

export const RenderChartView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    chart_type?: string;
    title?: string;
    x?: string;
    y?: string;
    series?: string;
    name?: string;
    value?: string;
    x_axis_label?: string;
    y_axis_label?: string;
  }>(item.toolInput);

  const output = parseToolJson<{ ok?: boolean; errors?: string[] }>(item.toolOutput);
  const ok = output?.ok ?? item.isStreaming;
  const errors = output?.errors ?? [];

  const chartFields = [
    { label: "Type", value: input?.chart_type ?? "—" },
    input?.title ? { label: "Title", value: input.title } : null,
    input?.x ? { label: "X", value: input.x } : null,
    input?.y ? { label: "Y", value: input.y } : null,
    input?.series ? { label: "Series", value: input.series } : null,
    input?.name ? { label: "Name", value: input.name } : null,
    input?.value ? { label: "Value", value: input.value } : null,
    input?.x_axis_label ? { label: "X Label", value: input.x_axis_label } : null,
    input?.y_axis_label ? { label: "Y Label", value: input.y_axis_label } : null
  ].filter(Boolean) as { label: string; value: string }[];

  return (
    <div className='overflow-auto p-4'>
      <div className='space-y-4'>
        <div
          className={`flex items-center gap-2 rounded border px-3 py-2 text-xs ${
            item.isStreaming
              ? "border-border bg-muted/30 text-muted-foreground"
              : ok
                ? "border-green-500/30 bg-green-500/5 text-green-700 dark:text-green-400"
                : "border-destructive/30 bg-destructive/5 text-destructive"
          }`}
        >
          {item.isStreaming ? (
            <span className='inline-block h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent' />
          ) : ok ? (
            <span>✓</span>
          ) : (
            <span>✕</span>
          )}
          <span className='font-medium'>
            {item.isStreaming ? "Rendering…" : ok ? "Chart rendered" : "Render failed"}
          </span>
        </div>
        <div className='grid grid-cols-2 gap-2'>
          {chartFields.map((f) => (
            <div key={f.label} className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>{f.label}</p>
              <p className='font-medium font-mono text-xs'>{f.value}</p>
            </div>
          ))}
        </div>
        {errors.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-destructive text-xs'>Errors</p>
            <div className='space-y-1.5'>
              {errors.map((e) => (
                <p
                  key={e}
                  className='rounded border border-destructive/30 bg-destructive/5 px-2.5 py-1.5 text-[11px] text-destructive'
                >
                  {e}
                </p>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

// ── ResolveSchemaView ─────────────────────────────────────────────────────────

export const ResolveSchemaView = ({ item }: { item: ArtifactItem }) => {
  // toolOutput is plain text: "Tables: table1, table2, ..."
  const raw = item.toolOutput ?? "";
  const tablesPart = raw.startsWith("Tables: ") ? raw.slice("Tables: ".length) : raw;
  const tables = tablesPart
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-3'>
        {tables.length > 0 ? (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>
              Tables ({tables.length})
            </p>
            <div className='flex flex-wrap gap-1.5'>
              {tables.map((t) => (
                <span key={t} className='rounded-full bg-muted px-2.5 py-0.5 font-mono text-xs'>
                  {t}
                </span>
              ))}
            </div>
          </div>
        ) : (
          <p className='text-muted-foreground text-xs'>{raw || "Schema resolved"}</p>
        )}
        {item.durationMs !== undefined && (
          <p className='font-mono text-[10px] text-muted-foreground/60'>{item.durationMs}ms</p>
        )}
      </div>
    </div>
  );
};

// ── ProcedureStepView ─────────────────────────────────────────────────────────
//
// Procedure steps set toolInput = "Running…" (a plain string, never JSON),
// which reliably identifies them vs. LLM tool calls that always get
// JSON.stringify(input).

export const ProcedureStepView = ({ item }: { item: ArtifactItem }) => {
  const isRunning = item.isStreaming;
  const isSuccess = !item.isStreaming && item.toolOutput === "Completed";
  const error = !item.isStreaming && item.toolOutput !== "Completed" ? item.toolOutput : undefined;

  return (
    <div className='flex h-full flex-col gap-4 p-4'>
      <div className='flex items-center gap-3 rounded-lg border bg-muted/30 p-4'>
        {isRunning && (
          <>
            <span className='inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent text-muted-foreground' />
            <span className='text-muted-foreground text-sm'>Running…</span>
          </>
        )}
        {isSuccess && (
          <>
            <span className='flex h-5 w-5 items-center justify-center rounded-full bg-green-500/15 text-green-600'>
              ✓
            </span>
            <span className='text-green-700 text-sm dark:text-green-400'>
              Completed successfully
            </span>
          </>
        )}
        {error !== undefined && (
          <>
            <span className='flex h-5 w-5 items-center justify-center rounded-full bg-destructive/15 text-destructive'>
              ✕
            </span>
            <span className='text-destructive text-sm'>Failed</span>
          </>
        )}
      </div>

      {error && (
        <div>
          <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Error</p>
          <pre className='whitespace-pre-wrap rounded border border-destructive/30 bg-destructive/5 p-3 font-mono text-[11px] text-destructive'>
            {error}
          </pre>
        </div>
      )}
    </div>
  );
};

// ── ColumnValuesView ──────────────────────────────────────────────────────────

export const ColumnValuesView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ table?: string; column?: string }>(item.toolInput);
  const table = input?.table ?? "?";
  const column = input?.column ?? "?";
  const output = parseToolJson<{ values?: unknown[]; error?: string }>(item.toolOutput);
  const values = output?.values ?? [];
  const error = output?.error;

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Table</p>
            <p className='font-medium font-mono text-xs'>{table}</p>
          </div>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Column</p>
            <p className='font-medium font-mono text-xs'>{column}</p>
          </div>
        </div>
        {error && (
          <p className='rounded border border-destructive/30 bg-destructive/5 px-2.5 py-1.5 text-[11px] text-destructive'>
            {error}
          </p>
        )}
        {values.length > 0 && (
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>
              Values ({values.length})
            </p>
            <div className='overflow-hidden rounded border'>
              <table className='w-full text-xs'>
                <thead>
                  <tr className='bg-muted/50'>
                    <th className='px-2.5 py-1.5 text-left font-medium text-muted-foreground'>
                      {column}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {values.map((v, i) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: stable ordered list
                    <tr key={i} className={i % 2 === 0 ? "bg-background" : "bg-muted/20"}>
                      <td className='px-2.5 py-1 font-mono'>{String(v ?? "null")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

// ── ColumnRangeView ───────────────────────────────────────────────────────────

export const ColumnRangeView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ table?: string; column?: string }>(item.toolInput);
  const table = input?.table ?? "?";
  const column = input?.column ?? "?";
  const output = parseToolJson<{
    min?: string;
    max?: string;
    distinct_count?: string;
    error?: string;
  }>(item.toolOutput);
  const error = output?.error;

  const stats = [
    { label: "Table", value: table },
    { label: "Column", value: column },
    output?.min !== undefined ? { label: "Min", value: output.min } : null,
    output?.max !== undefined ? { label: "Max", value: output.max } : null,
    output?.distinct_count !== undefined
      ? { label: "Distinct", value: output.distinct_count }
      : null,
  ].filter(Boolean) as { label: string; value: string }[];

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          {stats.map((s) => (
            <div key={s.label} className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>{s.label}</p>
              <p className='font-medium font-mono text-xs'>{s.value}</p>
            </div>
          ))}
        </div>
        {error && (
          <p className='rounded border border-destructive/30 bg-destructive/5 px-2.5 py-1.5 text-[11px] text-destructive'>
            {error}
          </p>
        )}
      </div>
    </div>
  );
};

// ── CountRowsView ─────────────────────────────────────────────────────────────

export const CountRowsView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ table?: string; filter?: string | null }>(item.toolInput);
  const table = input?.table ?? "?";
  const filter = typeof input?.filter === "string" ? input.filter : null;
  const output = parseToolJson<{ count?: string; error?: string }>(item.toolOutput);
  const count = output?.count;
  const error = output?.error;

  return (
    <div className='h-full overflow-auto p-4'>
      <div className='space-y-4'>
        <div className='grid grid-cols-2 gap-2'>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Table</p>
            <p className='font-medium font-mono text-xs'>{table}</p>
          </div>
          {count !== undefined && (
            <div className='rounded border bg-muted/30 px-2.5 py-2'>
              <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Count</p>
              <p className='font-medium font-mono text-xs'>
                {Number.isNaN(Number(count)) ? count : Number(count).toLocaleString()}
              </p>
            </div>
          )}
        </div>
        {filter && (
          <div>
            <p className='mb-1 font-medium text-muted-foreground text-xs'>Filter</p>
            <pre className='whitespace-pre-wrap rounded border bg-muted/50 p-2.5 font-mono text-[11px]'>
              {filter}
            </pre>
          </div>
        )}
        {error && (
          <p className='rounded border border-destructive/30 bg-destructive/5 px-2.5 py-1.5 text-[11px] text-destructive'>
            {error}
          </p>
        )}
      </div>
    </div>
  );
};
