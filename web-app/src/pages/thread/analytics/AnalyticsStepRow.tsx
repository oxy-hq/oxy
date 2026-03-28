import {
  BarChart2,
  BookOpen,
  Brain,
  Check,
  ChevronRight,
  Database,
  GitBranch,
  GitMerge,
  Info,
  Layers,
  Loader2,
  Search,
  Table2,
  Wrench
} from "lucide-react";
import { useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import type {
  AnalyticsStep,
  ArtifactItem,
  ProcedureItem,
  SelectableItem,
  SqlItem,
  StepLlmUsage,
  TextItem,
  ThinkingItem
} from "@/hooks/analyticsSteps";
import { cn } from "@/libs/shadcn/utils";

// ── LLM Usage Tooltip ────────────────────────────────────────────────────────

function formatTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

function LlmUsageTooltip({ usage }: { usage: StepLlmUsage }) {
  return (
    <div className='flex flex-col gap-1 font-mono text-[10px] leading-relaxed'>
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>Input tokens</span>
        <span>{formatTokens(usage.inputTokens)}</span>
      </div>
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>Output tokens</span>
        <span>{formatTokens(usage.outputTokens)}</span>
      </div>
      <div className='flex justify-between gap-4'>
        <span className='text-muted-foreground'>Wall time</span>
        <span>{(usage.durationMs / 1000).toFixed(1)}s</span>
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

// ── Child item renderers ──────────────────────────────────────────────────────

const ThinkingChild = ({ item, border }: { item: ThinkingItem; border: string }) => {
  const [expanded, setExpanded] = useState(true);
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
    // ── Clarifying ────────────────────────────────────────────────────────────
    case "search_catalog": {
      const queries = Array.isArray(input?.queries) ? (input.queries as string[]) : [];
      const preview = queries.length ? queries.join(", ") : "Search catalog";
      return { Icon: Search, label: "Search Catalog", preview: trunc(preview) };
    }
    case "get_metric_definition": {
      const metric = typeof input?.metric === "string" ? input.metric : "—";
      return { Icon: BookOpen, label: "Metric Definition", preview: metric };
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
    case "sample_column": {
      const table = typeof input?.table === "string" ? input.table : "?";
      const col = typeof input?.column === "string" ? input.column : "?";
      return { Icon: Table2, label: "Sample Column", preview: `${table}.${col}` };
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
          {item.durationMs}ms
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
        <span className='shrink-0 text-[10px] text-destructive'>Failed</span>
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

  return (
    <div>
      <button
        type='button'
        onClick={() => setExpanded((v) => !v)}
        className='w-full cursor-pointer text-left'
      >
        <div
          className={cn(
            "flex items-center gap-2 rounded-md border px-3 py-1.5 transition-all duration-200",
            isRunning
              ? cn("border-l-2", colors.border, "bg-secondary/80")
              : "border-transparent bg-card/50 hover:bg-card"
          )}
        >
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
