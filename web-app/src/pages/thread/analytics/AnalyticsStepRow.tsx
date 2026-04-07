import {
  BarChart2,
  BookOpen,
  Brain,
  Check,
  ChevronRight,
  Database,
  FilePen,
  FileText,
  FlaskConical,
  FolderSearch,
  GitBranch,
  GitMerge,
  Info,
  Layers,
  Loader2,
  MessageSquare,
  Search,
  ShieldCheck,
  Table2,
  TextSearch,
  Wrench
} from "lucide-react";
import { useMemo, useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import type {
  AnalyticsStep,
  ArtifactItem,
  ProcedureItem,
  SelectableItem,
  SqlItem,
  StepLlmUsage,
  TextItem,
  ThinkingItem,
  TraceItem
} from "@/hooks/analyticsSteps";
import { cn } from "@/libs/shadcn/utils";

// ── LLM Usage Tooltip ────────────────────────────────────────────────────────

function formatTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`;
}

function LlmUsageTooltip({ usage }: { usage: StepLlmUsage }) {
  const llmInferenceMs = Math.max(0, usage.durationMs - usage.toolDurationMs);
  return (
    <div className='flex flex-col gap-1 font-mono text-[10px] leading-relaxed'>
      {usage.model && (
        <div className='flex justify-between gap-4'>
          <span className='text-muted-foreground'>Model</span>
          <span>{usage.model}</span>
        </div>
      )}
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>Input tokens</span>
        <span>{formatTokens(usage.inputTokens)}</span>
      </div>
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>Output tokens</span>
        <span>{formatTokens(usage.outputTokens)}</span>
      </div>
      <div className='my-0.5 border-border/50 border-t' />
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>Total time</span>
        <span>{formatMs(usage.durationMs)}</span>
      </div>
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>↳ LLM inference</span>
        <span>{formatMs(llmInferenceMs)}</span>
      </div>
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>↳ Tool execution</span>
        <span>{formatMs(usage.toolDurationMs)}</span>
      </div>
    </div>
  );
}

// ── Colors per step label ─────────────────────────────────────────────────────

type Colors = { dot: string; icon: string; border: string; bg: string };

function stepColors(label: string): Colors {
  switch (label) {
    case "Analyzing":
    case "Answering":
      return {
        dot: "bg-node-agent",
        icon: "text-node-agent",
        border: "border-node-agent/30",
        bg: "bg-node-agent/8"
      };
    case "Planning":
      return {
        dot: "bg-node-plan",
        icon: "text-node-plan",
        border: "border-node-plan/30",
        bg: "bg-node-plan/8"
      };
    case "Running":
      return {
        dot: "bg-node-query",
        icon: "text-node-query",
        border: "border-node-query/30",
        bg: "bg-node-query/8"
      };
    default:
      return {
        dot: "bg-muted-foreground",
        icon: "text-muted-foreground",
        border: "border-muted-foreground/30",
        bg: "bg-muted/8"
      };
  }
}

// ── Artifact pill ────────────────────────────────────────────────────────────

const PILL_CLASS =
  "flex shrink-0 items-center gap-1 rounded bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground max-w-[120px]";

interface ArtifactPillProps {
  icon: React.FC<React.SVGProps<SVGSVGElement>>;
  label: string;
  onClick: () => void;
}

const ArtifactPill = ({ icon: Icon, label, onClick }: ArtifactPillProps) => (
  <button
    type='button'
    onClick={(e) => {
      e.stopPropagation();
      onClick();
    }}
    className={PILL_CLASS}
  >
    <Icon className='h-3 w-3 shrink-0' />
    <span className='truncate'>{label}</span>
  </button>
);

// ── Child item renderers ──────────────────────────────────────────────────────

const ThinkingChild = ({ item, border }: { item: ThinkingItem; border: string }) => {
  const [expanded, setExpanded] = useState(false);
  const preview = item.text
    ? item.text.slice(0, 60) + (item.text.length > 60 ? "…" : "")
    : "Thinking…";

  return (
    <div>
      <button
        type='button'
        onClick={() => setExpanded((v) => !v)}
        className='flex w-full items-center gap-1.5 py-0.5 text-left'
      >
        <Brain className='h-3 w-3 shrink-0 text-muted-foreground' />
        <span className='flex-1 truncate text-muted-foreground text-xs'>{preview}</span>
        {item.isStreaming && <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />}
        {!item.isStreaming && <Check className='h-3 w-3 shrink-0 text-primary/60' />}
        <ChevronRight
          className={cn(
            "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
            expanded && "rotate-90"
          )}
        />
      </button>
      <div
        className={cn(
          "overflow-hidden transition-all duration-200",
          expanded ? "opacity-100" : "max-h-0 opacity-0"
        )}
      >
        {item.text && (
          <div className={cn("mt-0.5 mb-1 ml-4 overflow-y-auto border-l-2 pl-2", border)}>
            <p className='whitespace-pre-wrap text-muted-foreground text-xs leading-relaxed'>
              {item.text}
            </p>
          </div>
        )}
      </div>
    </div>
  );
};

// ── ArtifactChild helpers ─────────────────────────────────────────────────────

type JsonObject = Record<string, unknown>;

function tryParseJson(raw: string): JsonObject | null {
  try {
    let v = JSON.parse(raw);
    // toolInput is JSON.stringify'd in analyticsSteps, so the payload may be
    // a JSON-encoded string that needs a second parse.
    if (typeof v === "string") v = JSON.parse(v);
    if (v !== null && typeof v === "object" && !Array.isArray(v)) return v as JsonObject;
  } catch {}
  return null;
}

function trunc(s: string, max = 60): string {
  return s.length > max ? `${s.slice(0, max)}…` : s;
}

type ToolDisplay = {
  Icon: React.FC<React.SVGProps<SVGSVGElement>>;
  label: string;
  preview: string;
};

function getToolDisplay(item: ArtifactItem): ToolDisplay {
  const input = tryParseJson(item.toolInput);

  switch (item.toolName) {
    // ── Ask user ──────────────────────────────────────────────────────────────
    case "ask_user": {
      const prompt = typeof input?.prompt === "string" ? input.prompt : "Asking user…";
      const output = tryParseJson(item.toolOutput ?? "");
      const answer = typeof output?.answer === "string" ? output.answer : null;
      const preview = answer ? `${trunc(prompt, 30)} → ${trunc(answer, 30)}` : trunc(prompt);
      return { Icon: MessageSquare, label: "Ask User", preview };
    }

    // ── Clarifying ────────────────────────────────────────────────────────────
    case "search_catalog": {
      const queries = Array.isArray(input?.queries) ? (input.queries as string[]) : [];
      const preview = queries.length ? queries.join(", ") : "Search catalog";
      return { Icon: Search, label: "Search Catalog", preview: trunc(preview) };
    }
    case "get_metric_definition": {
      const metric = typeof input?.metric === "string" ? input.metric : "—";
      return { Icon: Layers, label: "Metric Definition", preview: metric };
    }
    case "search_procedures": {
      const query = typeof input?.query === "string" ? input.query : "Search procedures";
      return { Icon: Search, label: "Search Procedures", preview: trunc(query) };
    }

    // ── Specifying ────────────────────────────────────────────────────────────
    case "get_join_path": {
      const from = typeof input?.from_entity === "string" ? input.from_entity : "?";
      const to = typeof input?.to_entity === "string" ? input.to_entity : "?";
      return { Icon: GitMerge, label: "Join Path", preview: `${from} → ${to}` };
    }
    case "sample_columns": {
      const cols = Array.isArray(input?.columns) ? input.columns : [];
      const preview = cols
        .map((c: Record<string, unknown>) => {
          const t = typeof c?.table === "string" ? c.table : "?";
          const col = typeof c?.column === "string" ? c.column : "?";
          return `${t}.${col}`;
        })
        .join(", ");
      return { Icon: Table2, label: "Sample Columns", preview: trunc(preview || "?") };
    }

    // ── Specifying (semantic compile) ─────────────────────────────────────────
    case "compile_semantic_query": {
      const output = tryParseJson(item.toolOutput ?? "");
      const success = output?.success !== false;
      const preview = success
        ? "Compiled"
        : typeof output?.error === "string"
          ? output.error
          : "Compile failed";
      return { Icon: GitBranch, label: "Compile Semantic Query", preview: trunc(preview) };
    }

    // ── Solving ───────────────────────────────────────────────────────────────
    case "execute_preview": {
      const sql = typeof input?.sql === "string" ? (input.sql.trim().split("\n")[0] ?? "") : "";
      return { Icon: Database, label: "Preview Query", preview: trunc(sql) };
    }

    // ── Interpreting ──────────────────────────────────────────────────────────
    case "render_chart": {
      const chartType = typeof input?.chart_type === "string" ? input.chart_type : "chart";
      const title = typeof input?.title === "string" ? input.title : null;
      const preview = title ? `${title} · ${chartType}` : chartType;
      return { Icon: BarChart2, label: "Render Chart", preview: trunc(preview) };
    }

    // ── Builder tools ──────────────────────────────────────────────────────────
    case "propose_change": {
      const filePath = typeof input?.file_path === "string" ? input.file_path : "?";
      const isDelete = input?.delete === true;
      const hasOldContent = typeof input?.old_content === "string";
      const action = isDelete ? "Delete" : hasOldContent ? "Update" : "Create";
      const output = tryParseJson(item.toolOutput ?? "");
      const status =
        typeof output?.status === "string" && output.status !== "awaiting_response"
          ? output.status
          : null;
      const preview = status
        ? `${action} ${trunc(filePath, 30)} · ${status}`
        : `${action} ${trunc(filePath, 40)}`;
      return { Icon: FilePen, label: "Propose Change", preview };
    }

    case "read_file": {
      const filePath =
        typeof input?.file_path === "string"
          ? input.file_path
          : typeof input?.path === "string"
            ? input.path
            : "?";
      const output = tryParseJson(item.toolOutput ?? "");
      const totalLines = typeof output?.total_lines === "number" ? output.total_lines : null;
      const preview =
        totalLines !== null ? `${trunc(filePath, 35)} · ${totalLines} lines` : trunc(filePath);
      return { Icon: FileText, label: "Read File", preview };
    }

    case "search_files": {
      const pattern = typeof input?.pattern === "string" ? input.pattern : "?";
      const output = tryParseJson(item.toolOutput ?? "");
      const count =
        typeof output?.count === "number"
          ? output.count
          : Array.isArray(output?.files)
            ? output.files.length
            : null;
      const preview = count !== null ? `${trunc(pattern, 35)} · ${count} files` : trunc(pattern);
      return { Icon: FolderSearch, label: "Search Files", preview };
    }

    case "lookup_schema": {
      const objectName = typeof input?.object_name === "string" ? input.object_name : "?";
      return { Icon: BookOpen, label: "Lookup Schema", preview: trunc(objectName) };
    }

    case "run_tests": {
      const filePath = typeof input?.file_path === "string" ? input.file_path : null;
      const scope = filePath ? trunc(filePath, 35) : "All tests";
      const output = tryParseJson(item.toolOutput ?? "");
      const testsRun = typeof output?.tests_run === "number" ? output.tests_run : null;
      const preview = testsRun !== null ? `${scope} · ${testsRun} run` : scope;
      return { Icon: FlaskConical, label: "Run Tests", preview };
    }

    case "validate_project": {
      const filePath = typeof input?.file_path === "string" ? input.file_path : null;
      const scope = filePath ? trunc(filePath, 35) : "Whole project";
      const output = tryParseJson(item.toolOutput ?? "");
      const isValid = output?.valid === true;
      const errorCount =
        typeof output?.error_count === "number"
          ? output.error_count
          : Array.isArray(output?.errors)
            ? output.errors.length
            : null;
      const status =
        output !== null
          ? isValid
            ? "Valid"
            : errorCount !== null
              ? `${errorCount} errors`
              : "Errors found"
          : null;
      const preview = status ? `${scope} · ${status}` : scope;
      return { Icon: ShieldCheck, label: "Validate Project", preview };
    }

    case "search_text": {
      const pattern = typeof input?.pattern === "string" ? input.pattern : "?";
      const fileGlob = typeof input?.file_glob === "string" ? input.file_glob : null;
      const output = tryParseJson(item.toolOutput ?? "");
      const count = typeof output?.count === "number" ? output.count : null;
      const truncated = output?.truncated === true;
      const scope = fileGlob ? ` in ${fileGlob}` : "";
      const preview =
        count !== null
          ? `${trunc(pattern, 30)}${scope} · ${count}${truncated ? "+" : ""} matches`
          : `${trunc(pattern, 40)}${scope}`;
      return { Icon: TextSearch, label: "Search Text", preview };
    }

    case "execute_sql": {
      const db = typeof input?.database === "string" ? input.database : null;
      const output = tryParseJson(item.toolOutput ?? "");
      const rowCount = typeof output?.row_count === "number" ? output.row_count : null;
      const parts = [db ?? "SQL", rowCount !== null ? `${rowCount} rows` : null].filter(Boolean);
      return { Icon: Database, label: "Execute SQL", preview: trunc(parts.join(" · ")) };
    }

    // ── Domain event ──────────────────────────────────────────────────────────
    case "resolve_schema": {
      const tables = item.toolOutput ?? "Resolving schema…";
      return { Icon: Layers, label: "Schema", preview: trunc(tables) };
    }

    // ── Fallback ──────────────────────────────────────────────────────────────
    default: {
      let preview: string;
      try {
        const s = JSON.stringify(JSON.parse(item.toolInput));
        preview = s.length > 50 ? `${s.slice(0, 50)}…` : s;
      } catch {
        preview = item.toolInput.slice(0, 50);
      }
      return { Icon: Wrench, label: item.toolName, preview };
    }
  }
}

const ArtifactChild = ({
  item,
  onSelect
}: {
  item: ArtifactItem;
  onSelect: (item: SelectableItem) => void;
}) => {
  const { Icon, label, preview } = getToolDisplay(item);

  return (
    <button
      type='button'
      onClick={() => onSelect(item)}
      className='flex w-full items-center gap-1.5 py-0.5 text-left transition-opacity hover:opacity-70'
    >
      <Icon className='h-3 w-3 shrink-0 text-muted-foreground' />
      <span className='shrink-0 font-medium text-muted-foreground text-xs'>{label}</span>
      <span className='flex-1 truncate text-muted-foreground/60 text-xs'>{preview}</span>
      {item.isStreaming && <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />}
      {!item.isStreaming && item.durationMs !== undefined && (
        <span className='shrink-0 font-mono text-[10px] text-muted-foreground/60'>
          {formatMs(item.durationMs)}
        </span>
      )}
      <ChevronRight className='h-3 w-3 shrink-0 text-muted-foreground' />
    </button>
  );
};

const SqlChild = ({
  item,
  onSelect
}: {
  item: SqlItem;
  onSelect: (item: SelectableItem) => void;
}) => {
  const preview = item.sql.trim().split("\n")[0];
  const truncated = preview.length > 60 ? `${preview.slice(0, 60)}…` : preview;

  return (
    <button
      type='button'
      onClick={() => onSelect(item)}
      className='flex w-full items-center gap-1.5 py-0.5 text-left transition-opacity hover:opacity-70'
    >
      <Database className='h-3 w-3 shrink-0 text-muted-foreground' />
      <span className='flex-1 truncate font-mono text-muted-foreground text-xs'>{truncated}</span>
      {item.isStreaming && <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />}
      {!item.isStreaming && item.error && (
        <span className='shrink-0 text-[10px] text-destructive'>Failed: {item.error}</span>
      )}
      {!item.isStreaming && !item.error && item.rowCount !== undefined && (
        <span className='shrink-0 font-mono text-[10px] text-muted-foreground/60'>
          {item.rowCount} rows
        </span>
      )}
      <ChevronRight className='h-3 w-3 shrink-0 text-muted-foreground' />
    </button>
  );
};

const ProcedureChild = ({
  item,
  onSelect
}: {
  item: ProcedureItem;
  onSelect: (item: SelectableItem) => void;
}) => (
  <button
    type='button'
    onClick={() => onSelect(item)}
    className='flex w-full items-center gap-1.5 py-0.5 text-left transition-opacity hover:opacity-70'
  >
    <GitBranch className='h-3 w-3 shrink-0 text-muted-foreground' />
    <span className='shrink-0 font-medium text-muted-foreground text-xs'>{item.procedureName}</span>
    <span className='flex-1 font-mono text-muted-foreground/60 text-xs'>
      {item.stepsDone}/{item.steps.length}
    </span>
    <ChevronRight className='h-3 w-3 shrink-0 text-muted-foreground' />
  </button>
);

const TextChild = ({ item }: { item: TextItem }) => (
  <p className='whitespace-pre-wrap text-muted-foreground text-xs leading-relaxed'>
    {item.text}
    {item.isStreaming && (
      <span className='ml-0.5 inline-block h-3 w-0.5 animate-pulse bg-foreground' />
    )}
  </p>
);

// ── Step row ──────────────────────────────────────────────────────────────────

type PillInfo = {
  id: string;
  icon: React.FC<React.SVGProps<SVGSVGElement>>;
  label: string;
  item: SelectableItem;
};

function collectPills(items: TraceItem[]): PillInfo[] {
  const pills: PillInfo[] = [];
  for (const item of items) {
    if (item.kind === "artifact") {
      const { Icon, label } = getToolDisplay(item);
      pills.push({ id: item.id, icon: Icon, label, item });
    } else if (item.kind === "sql") {
      const firstLine = item.sql.trim().split("\n")[0] ?? "SQL";
      const label = firstLine.length > 20 ? `${firstLine.slice(0, 20)}…` : firstLine;
      pills.push({ id: item.id, icon: Database, label, item });
    } else if (item.kind === "procedure") {
      pills.push({ id: item.id, icon: GitBranch, label: item.procedureName, item });
    }
  }
  return pills;
}

interface AnalyticsStepRowProps {
  step: AnalyticsStep;
  onSelectArtifact: (item: SelectableItem) => void;
}

const AnalyticsStepRow = ({ step, onSelectArtifact }: AnalyticsStepRowProps) => {
  const [expanded, setExpanded] = useState(step.isStreaming);
  const colors = stepColors(step.label);
  const isRunning = step.isStreaming;
  const hasError = !!step.error;
  const isDone = !isRunning && !hasError;
  const pills = useMemo(() => collectPills(step.items), [step.items]);

  return (
    <div>
      <button
        type='button'
        onClick={() => setExpanded((v) => !v)}
        className='w-full cursor-pointer text-left'
      >
        <div
          className={cn(
            "rounded-md border px-3 py-1.5 transition-all duration-200",
            isRunning
              ? cn("border-l-2", colors.border, "bg-secondary/80")
              : "border-transparent bg-card/50 hover:bg-card"
          )}
        >
          <div className='flex items-center gap-2'>
            <div
              className={cn(
                "h-1.5 w-1.5 shrink-0 rounded-full transition-all duration-200",
                colors.dot,
                isRunning && "animate-pulse",
                !isRunning && "opacity-30"
              )}
            />
            <div className='flex min-w-0 flex-1 flex-col'>
              <span
                className={cn(
                  "text-sm transition-colors duration-200",
                  isRunning ? "text-foreground" : "text-muted-foreground"
                )}
              >
                {step.summary || step.label}
              </span>
            </div>
            {isRunning && <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />}
            {isDone && <Check className='h-3 w-3 shrink-0 text-primary' />}
            {hasError && <span className='shrink-0 text-destructive text-xs'>Error</span>}
            {isDone && step.llmUsage && (
              <Tooltip>
                <TooltipTrigger asChild onClick={(e) => e.stopPropagation()}>
                  <Info className='h-3 w-3 shrink-0 cursor-pointer text-muted-foreground/50 hover:text-muted-foreground' />
                </TooltipTrigger>
                <TooltipContent
                  side='left'
                  className='max-w-xs bg-popover p-2 text-popover-foreground'
                >
                  <LlmUsageTooltip usage={step.llmUsage} />
                </TooltipContent>
              </Tooltip>
            )}
            <ChevronRight
              className={cn(
                "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
                expanded && "rotate-90"
              )}
            />
          </div>
          {pills.length > 0 && (
            <div className='mt-1.5 flex flex-wrap items-center gap-1 pl-3.5'>
              {pills.map((pill) => (
                <ArtifactPill
                  key={pill.id}
                  icon={pill.icon}
                  label={pill.label}
                  onClick={() => onSelectArtifact(pill.item)}
                />
              ))}
            </div>
          )}
        </div>
      </button>

      <div
        className={cn(
          "overflow-hidden transition-all duration-300",
          expanded ? "opacity-100" : "max-h-0 opacity-0"
        )}
      >
        {step.items.length > 0 && (
          <div className={cn("mt-1 mb-2 ml-8 space-y-1.5 border-l-2 pl-3", colors.border)}>
            {step.items.map((item) => {
              if (item.kind === "thinking")
                return <ThinkingChild key={item.id} item={item} border={colors.border} />;
              if (item.kind === "artifact")
                return <ArtifactChild key={item.id} item={item} onSelect={onSelectArtifact} />;
              if (item.kind === "sql")
                return <SqlChild key={item.id} item={item} onSelect={onSelectArtifact} />;
              if (item.kind === "procedure")
                return <ProcedureChild key={item.id} item={item} onSelect={onSelectArtifact} />;
              return <TextChild key={item.id} item={item} />;
            })}
          </div>
        )}
        {step.error && <p className='mt-1 ml-8 text-destructive text-xs'>{step.error}</p>}
      </div>
    </div>
  );
};

export default AnalyticsStepRow;
