import ErrorAlert from "@/components/ui/ErrorAlert";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import type { ArtifactItem } from "@/hooks/analyticsSteps";
import type { AnalyticsDisplayBlock } from "@/hooks/useAnalyticsRun";
import { AnalyticsDisplayBlockItem, parseToolJson } from "./analyticsArtifactHelpers";

// ── TimingBar ─────────────────────────────────────────────────────────────────

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`;
}

const TimingBar = ({ item }: { item: ArtifactItem }) => {
  const execMs = item.durationMs;
  const llmMs = item.llmDurationMs;
  if (execMs === undefined && llmMs === undefined) return null;
  return (
    <div className='flex items-center gap-3 border-t px-4 py-2 font-mono text-[10px] text-muted-foreground/70'>
      {llmMs !== undefined && (
        <span>
          <span className='text-muted-foreground/50'>llm </span>
          {formatMs(llmMs)}
        </span>
      )}
      {execMs !== undefined && (
        <span>
          <span className='text-muted-foreground/50'>exec </span>
          {formatMs(execMs)}
        </span>
      )}
      {llmMs !== undefined && execMs !== undefined && (
        <span className='ml-auto'>
          <span className='text-muted-foreground/50'>total </span>
          {formatMs(llmMs + execMs)}
        </span>
      )}
    </div>
  );
};

// ── ChartSection ──────────────────────────────────────────────────────────────

export const ChartSection = ({
  displayBlocks,
  runId = "sidebar"
}: {
  displayBlocks: AnalyticsDisplayBlock[];
  runId?: string;
}) => {
  if (!displayBlocks.length) return null;
  return (
    <div className='shrink-0 space-y-2 border-t p-4'>
      {displayBlocks.map((block, i) => {
        const key = `${block.config.chart_type}-${block.config.title ?? i}`;
        return <AnalyticsDisplayBlockItem key={key} block={block} index={i} runId={runId} />;
      })}
    </div>
  );
};

// ── RawArtifactView ───────────────────────────────────────────────────────────

export const RawArtifactView = ({ item }: { item: ArtifactItem }) => (
  <div className='flex h-full flex-col'>
    <div className='flex-1 overflow-auto p-4'>
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
    <TimingBar item={item} />
  </div>
);

// ── AskUserView ──────────────────────────────────────────────────────────────

export const AskUserView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ prompt?: string; suggestions?: string[] }>(item.toolInput);
  const prompt = input?.prompt ?? "Question";
  const suggestions = input?.suggestions ?? [];

  const output = parseToolJson<{ answer?: string }>(item.toolOutput);
  const answer = output?.answer ?? null;

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
        <div className='space-y-4'>
          <div>
            <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Question</p>
            <p className='text-sm'>{prompt}</p>
          </div>

          {suggestions.length > 0 && (
            <div>
              <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Suggestions</p>
              <div className='flex flex-wrap gap-1.5'>
                {suggestions.map((s) => (
                  <span
                    key={s}
                    className='rounded-full border border-border bg-muted/50 px-2.5 py-0.5 text-xs'
                  >
                    {s}
                  </span>
                ))}
              </div>
            </div>
          )}

          {answer && (
            <div>
              <p className='mb-1.5 font-medium text-muted-foreground text-xs'>User Response</p>
              <div className='rounded border border-primary/20 bg-primary/5 px-3 py-2'>
                <p className='whitespace-pre-wrap text-sm'>{answer}</p>
              </div>
            </div>
          )}

          {!answer && item.isStreaming && (
            <p className='text-muted-foreground text-xs'>Waiting for user response…</p>
          )}
        </div>
      </div>
      <TimingBar item={item} />
    </div>
  );
};

// ── SearchCatalogView ─────────────────────────────────────────────────────────

type CatalogMetric = { name: string; description?: string };
type CatalogDimension = { name: string; description?: string; type?: string };

export const SearchCatalogView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{ queries?: string[] }>(item.toolInput);
  const queries = input?.queries ?? [];

  const output = parseToolJson<{
    metrics?: CatalogMetric[];
    dimensions?: CatalogDimension[];
  }>(item.toolOutput);
  const metrics = output?.metrics ?? [];
  const dimensions = output?.dimensions ?? [];

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
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
                {dimensions.map((d) => (
                  <div
                    key={d.name}
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
      <TimingBar item={item} />
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

interface ColumnResult {
  table?: string;
  column?: string;
  data_type?: string;
  sample_values?: unknown[];
  row_count?: number;
  distinct_count?: number;
  min?: string | number;
  max?: string | number;
  avg?: number;
  stdev?: number;
  error?: string;
}

const SingleColumnCard = ({ col }: { col: ColumnResult }) => {
  const table = col.table ?? "";
  const column = col.column ?? "";
  const dataType = col.data_type ?? "";
  const sampleValues = col.sample_values ?? [];

  const fmt = (v: string | number) =>
    typeof v === "number" ? v.toLocaleString(undefined, { maximumFractionDigits: 4 }) : v;

  if (col.error) {
    return (
      <div className='space-y-2'>
        <p className='font-medium font-mono text-xs'>
          {table}.{column}
        </p>
        <p className='text-destructive text-xs'>{col.error}</p>
      </div>
    );
  }

  const meta = [
    { label: "Table", value: table },
    { label: "Column", value: column },
    { label: "Type", value: dataType || "—" },
    col.row_count !== undefined
      ? { label: "Row Count", value: col.row_count.toLocaleString() }
      : null,
    col.distinct_count !== undefined
      ? { label: "Distinct", value: col.distinct_count.toLocaleString() }
      : null,
    col.min !== undefined ? { label: "Min", value: fmt(col.min) } : null,
    col.max !== undefined ? { label: "Max", value: fmt(col.max) } : null,
    col.avg !== undefined ? { label: "Avg", value: fmt(col.avg) } : null,
    col.stdev !== undefined ? { label: "Stdev", value: fmt(col.stdev) } : null
  ].filter(Boolean) as { label: string; value: string | number }[];

  return (
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
  );
};

export const SampleColumnView = ({ item }: { item: ArtifactItem }) => {
  const output = parseToolJson<{ results?: ColumnResult[] }>(item.toolOutput);
  const results = output?.results ?? [];

  if (results.length === 0) {
    return (
      <div className='flex h-full flex-col'>
        <div className='flex-1 p-4'>
          <p className='text-muted-foreground text-xs'>No column results.</p>
        </div>
        <TimingBar item={item} />
      </div>
    );
  }

  const tabLabel = (col: ColumnResult, i: number) => {
    const table = col.table || "?";
    const column = col.column || "?";
    return `${table}.${column}` === "?.?" ? `col-${i}` : `${table}.${column}`;
  };

  return (
    <div className='flex h-full flex-col'>
      <Tabs
        key={item.id}
        defaultValue={tabLabel(results[0], 0)}
        className='flex min-h-0 flex-1 flex-col'
      >
        <TabsList variant='line' className='shrink-0 overflow-x-auto border-b px-4 pt-2'>
          {results.map((col, i) => (
            <TabsTrigger key={tabLabel(col, i)} value={tabLabel(col, i)} className='text-xs'>
              {tabLabel(col, i)}
            </TabsTrigger>
          ))}
        </TabsList>
        {results.map((col, i) => (
          <TabsContent
            key={tabLabel(col, i)}
            value={tabLabel(col, i)}
            className='min-h-0 flex-1 overflow-auto p-4'
          >
            <SingleColumnCard col={col} />
          </TabsContent>
        ))}
      </Tabs>
      <TimingBar item={item} />
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
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
        <div className='space-y-4'>
          <div className='grid grid-cols-2 gap-2'>
            {fields.map((f) => (
              <div key={f.label} className='rounded border bg-muted/30 px-2.5 py-2'>
                <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>
                  {f.label}
                </p>
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
      <TimingBar item={item} />
    </div>
  );
};

// ── GetJoinPathView ───────────────────────────────────────────────────────────

export const GetJoinPathView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    from_entity?: string;
    to_entity?: string;
    from?: string;
    to?: string;
  }>(item.toolInput);
  // analytics uses from_entity/to_entity; app-builder uses from/to
  const from = input?.from_entity ?? input?.from ?? "?";
  const to = input?.to_entity ?? input?.to ?? "?";

  const output = parseToolJson<{ path?: string; join_type?: string }>(item.toolOutput);

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
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
      <TimingBar item={item} />
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
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
        <div className='space-y-4'>
          <div
            className={`flex items-center gap-2 rounded border px-3 py-2 text-xs ${
              item.isStreaming
                ? "border-border bg-muted/30 text-muted-foreground"
                : ok
                  ? "border-success/30 bg-success/5 text-success"
                  : "border-destructive/30 bg-destructive/5 text-destructive"
            }`}
          >
            {item.isStreaming ? (
              <Spinner className='size-3' />
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
                <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>
                  {f.label}
                </p>
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
      <TimingBar item={item} />
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
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
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
        </div>
      </div>
      <TimingBar item={item} />
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
            <Spinner className='text-muted-foreground' />
            <span className='text-muted-foreground text-sm'>Running…</span>
          </>
        )}
        {isSuccess && (
          <>
            <span className='flex h-5 w-5 items-center justify-center rounded-full bg-success/15 text-success'>
              ✓
            </span>
            <span className='text-sm text-success'>Completed successfully</span>
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
          <ErrorAlert
            message={<pre className='whitespace-pre-wrap font-mono text-xs'>{error}</pre>}
          />
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
        {error && <ErrorAlert message={error} />}
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
      : null
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
        {error && <ErrorAlert message={error} />}
      </div>
    </div>
  );
};

// ── CompileSemanticQueryView ──────────────────────────────────────────────────

export const CompileSemanticQueryView = ({ item }: { item: ArtifactItem }) => {
  const input = parseToolJson<{
    measures?: string[];
    dimensions?: string[];
    filters?: unknown[];
    time_dimensions?: unknown[];
  }>(item.toolInput);
  const output = parseToolJson<{ success?: boolean; sql?: string; error?: string }>(
    item.toolOutput
  );
  const success = output?.success !== false;

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
        <div className='space-y-4'>
          {(input?.measures?.length ?? 0) > 0 && (
            <div>
              <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Measures</p>
              <div className='flex flex-wrap gap-1'>
                {(input?.measures ?? []).map((m) => (
                  <span
                    key={m}
                    className='rounded border bg-muted/50 px-1.5 py-0.5 font-mono text-[11px]'
                  >
                    {m}
                  </span>
                ))}
              </div>
            </div>
          )}
          {(input?.dimensions?.length ?? 0) > 0 && (
            <div>
              <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Dimensions</p>
              <div className='flex flex-wrap gap-1'>
                {(input?.dimensions ?? []).map((d) => (
                  <span
                    key={d}
                    className='rounded border bg-muted/50 px-1.5 py-0.5 font-mono text-[11px]'
                  >
                    {d}
                  </span>
                ))}
              </div>
            </div>
          )}
          {Array.isArray(input?.filters) && input.filters.length > 0 && (
            <div>
              <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Filters</p>
              <div className='flex flex-wrap gap-1'>
                {(input.filters as Record<string, unknown>[]).map((f, i) => {
                  // Handle both formats: { field, op, value } and { member, operator, values }
                  const field = String(f.field ?? f.member ?? "");
                  const op = String(f.op ?? f.operator ?? "");
                  const rawVal = f.value ?? f.values;
                  const val = Array.isArray(rawVal)
                    ? rawVal.join(", ")
                    : rawVal != null
                      ? String(rawVal)
                      : "";
                  return (
                    <span
                      key={`${field}-${op}-${i}`}
                      className='inline-flex items-center gap-1 rounded border bg-muted/50 px-1.5 py-0.5 font-mono text-[11px]'
                    >
                      <span className='font-medium'>{field.split(".").pop()}</span>
                      <span className='text-muted-foreground'>{op}</span>
                      <span>{val}</span>
                    </span>
                  );
                })}
              </div>
            </div>
          )}
          {success && output?.sql && (
            <div>
              <p className='mb-1.5 font-medium text-muted-foreground text-xs'>Compiled SQL</p>
              <pre className='whitespace-pre-wrap rounded border bg-muted/50 p-3 font-mono text-[11px]'>
                {output.sql}
              </pre>
            </div>
          )}
          {!success && output?.error && (
            <p className='rounded border border-destructive/30 bg-destructive/5 px-2.5 py-1.5 text-[11px] text-destructive'>
              {output.error}
            </p>
          )}
        </div>
      </div>
      <TimingBar item={item} />
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
        {error && <ErrorAlert message={error} />}
      </div>
    </div>
  );
};
