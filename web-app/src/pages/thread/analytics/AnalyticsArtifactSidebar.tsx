import Editor from "@monaco-editor/react";
import { BadgeCheck } from "lucide-react";
import SqlArtifactPanel from "@/components/ArtifactPanel/ArtifactsContent/sql";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import type { ArtifactItem, ProcedureItem, SqlItem } from "@/hooks/analyticsSteps";
import type { AnalyticsDisplayBlock, SseEvent } from "@/hooks/useAnalyticsRun";
import { extractDisplayBlockForSeq } from "@/hooks/useAnalyticsRun";
import ProcedureRunDagPanel from "../agentic/ProcedureRunDagPanel";
import { VERIFIED_TOOLTIP } from "../constants";
import {
  AskUserView,
  ChartSection,
  ColumnRangeView,
  ColumnValuesView,
  CompileSemanticQueryView,
  CountRowsView,
  GetJoinPathView,
  GetMetricDefinitionView,
  ProcedureStepView,
  RawArtifactView,
  RenderChartView,
  ResolveSchemaView,
  SampleColumnView,
  SearchCatalogView,
  SearchProceduresView
} from "./AnalyticsArtifactViews";
import {
  sqlArtifactFromExecutePreview,
  sqlArtifactFromExecuteSql,
  sqlArtifactFromPreviewData,
  sqlArtifactFromSemanticQuery,
  sqlArtifactFromSqlItem
} from "./analyticsArtifactHelpers";
import {
  ExecuteSqlView,
  LookupSchemaView,
  ProposeChangeToolView,
  ReadFileView,
  RunTestsView,
  SearchFilesView,
  SemanticQueryView,
  ValidateProjectView,
  VerifiedSemanticQueryView
} from "./BuilderArtifactViews";

interface Props {
  item: ArtifactItem | SqlItem | ProcedureItem;
  displayBlocks?: AnalyticsDisplayBlock[];
  runEvents?: SseEvent[];
  isRunning?: boolean;
  onClose: () => void;
}

const AnalyticsArtifactSidebar = ({
  item,
  displayBlocks = [],
  runEvents = [],
  isRunning = false,
  onClose
}: Props) => {
  // ── kind === "procedure" → full DAG panel ─────────────────────────────────
  if (item.kind === "procedure") {
    return (
      <ProcedureRunDagPanel
        procedureName={item.procedureName}
        steps={item.steps}
        events={runEvents}
        isRunning={isRunning}
        onClose={onClose}
      />
    );
  }

  // ── kind === "sql" (query_executed domain event) ──────────────────────────
  if (item.kind === "sql") {
    const verified = item.source === "semantic";
    const title = (
      <div className='flex min-w-0 items-center gap-1.5'>
        <h3 className='truncate font-semibold text-sm'>
          {verified ? "Semantic Query" : "SQL Query"}
        </h3>
        {verified && (
          <Tooltip>
            <TooltipTrigger asChild>
              <BadgeCheck className='h-4 w-4 shrink-0 text-special' />
            </TooltipTrigger>
            <TooltipContent side='bottom'>{VERIFIED_TOOLTIP}</TooltipContent>
          </Tooltip>
        )}
      </div>
    );
    const subtitle =
      item.rowCount !== undefined ? `${item.rowCount} rows · ${item.durationMs ?? 0}ms` : undefined;

    if (verified && item.semanticQuery) {
      const sqlLineCount = item.sql.split("\n").length;
      const sqlHeight = Math.min(Math.max(sqlLineCount * 18 + 24, 120), 320);
      return (
        <Panel>
          <PanelHeader title={title} subtitle={subtitle} onClose={onClose} />
          <PanelContent scrollable={true} padding={false} className='flex min-h-0 flex-col'>
            <VerifiedSemanticQueryView query={item.semanticQuery} database={item.database} />
            <div className='border-t'>
              <div className='px-4 pt-3 pb-1.5 font-medium text-muted-foreground text-xs uppercase tracking-wide'>
                Compiled SQL
              </div>
              <div style={{ height: sqlHeight }}>
                <Editor
                  height='100%'
                  width='100%'
                  theme='vs-dark'
                  defaultValue={item.sql}
                  language='sql'
                  value={item.sql}
                  loading={<Spinner />}
                  options={{
                    readOnly: true,
                    scrollBeyondLastLine: false,
                    automaticLayout: true,
                    minimap: { enabled: false }
                  }}
                />
              </div>
            </div>
            {item.error && (
              <ErrorAlert className='mx-3 my-2 max-h-32 overflow-y-auto' message={item.error} />
            )}
            {!item.error && item.result && (
              <div className='border-t'>
                <div className='px-4 pt-3 pb-1.5 font-medium text-muted-foreground text-xs uppercase tracking-wide'>
                  Results
                </div>
                <SqlResultsTable result={item.result} />
              </div>
            )}
          </PanelContent>
        </Panel>
      );
    }

    return (
      <Panel>
        <PanelHeader title={title} subtitle={subtitle} onClose={onClose} />
        <PanelContent scrollable={false} padding={false} className='flex min-h-0 flex-col'>
          <div className='min-h-0 flex-1'>
            <SqlArtifactPanel artifact={sqlArtifactFromSqlItem(item)} />
          </div>
        </PanelContent>
      </Panel>
    );
  }

  // ── execute_preview → SQL panel ───────────────────────────────────────────
  if (item.toolName === "execute_preview") {
    const sqlArtifact = sqlArtifactFromExecutePreview(item);
    return (
      <Panel>
        <PanelHeader
          title='Preview Query'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          {sqlArtifact ? (
            <SqlArtifactPanel artifact={sqlArtifact} />
          ) : (
            <RawArtifactView item={item} />
          )}
        </PanelContent>
      </Panel>
    );
  }

  // ── render_chart → config + rendered chart ────────────────────────────────
  if (item.toolName === "render_chart") {
    const block =
      item.seq != null
        ? extractDisplayBlockForSeq(runEvents, item.seq)
        : (displayBlocks[0] ?? null);
    return (
      <Panel>
        <PanelHeader
          title='Render Chart'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='flex min-h-0 flex-col'>
          <div className='min-h-0 flex-1 overflow-auto'>
            <RenderChartView item={item} />
          </div>
          <ChartSection displayBlocks={block ? [block] : []} />
        </PanelContent>
      </Panel>
    );
  }

  // ── ask_user → question + user response ──────────────────────────────────
  if (item.toolName === "ask_user") {
    return (
      <Panel>
        <PanelHeader
          title='Ask User'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <AskUserView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── search_catalog → structured catalog view ──────────────────────────────
  if (item.toolName === "search_catalog") {
    return (
      <Panel>
        <PanelHeader
          title='Catalog Search'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <SearchCatalogView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "search_files") {
    return (
      <Panel>
        <PanelHeader
          title='Search Files'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <SearchFilesView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "preview_data") {
    const sqlArtifact = sqlArtifactFromPreviewData(item);
    return (
      <Panel>
        <PanelHeader
          title='Preview Table'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          {sqlArtifact ? (
            <SqlArtifactPanel artifact={sqlArtifact} />
          ) : (
            <RawArtifactView item={item} />
          )}
        </PanelContent>
      </Panel>
    );
  }

  // ── search_procedures → procedure list ───────────────────────────────────
  if (item.toolName === "search_procedures") {
    return (
      <Panel>
        <PanelHeader
          title='Procedure Search'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <SearchProceduresView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "read_file") {
    return (
      <Panel>
        <PanelHeader
          title='Read File'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ReadFileView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "execute_sql") {
    const sqlArtifact = sqlArtifactFromExecuteSql(item);
    return (
      <Panel>
        <PanelHeader
          title='Execute SQL'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='flex min-h-0 flex-col'>
          <div className='shrink-0'>
            <ExecuteSqlView item={item} />
          </div>
          <div className='min-h-0 flex-1'>
            {sqlArtifact ? (
              <SqlArtifactPanel artifact={sqlArtifact} />
            ) : (
              <RawArtifactView item={item} />
            )}
          </div>
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "propose_change") {
    return (
      <Panel>
        <PanelHeader
          title='Proposed Change'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ProposeChangeToolView item={item} runEvents={runEvents} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "lookup_schema") {
    return (
      <Panel>
        <PanelHeader
          title='Lookup Schema'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <LookupSchemaView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "semantic_query") {
    const sqlArtifact = sqlArtifactFromSemanticQuery(item);
    return (
      <Panel>
        <PanelHeader
          title='Semantic Query'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='flex min-h-0 flex-col'>
          <div className='shrink-0'>
            <SemanticQueryView item={item} />
          </div>
          <div className='min-h-0 flex-1'>
            {sqlArtifact ? (
              <SqlArtifactPanel artifact={sqlArtifact} />
            ) : (
              <RawArtifactView item={item} />
            )}
          </div>
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "validate_project") {
    return (
      <Panel>
        <PanelHeader
          title='Validate Project'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ValidateProjectView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "run_tests") {
    return (
      <Panel>
        <PanelHeader
          title='Run Tests'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <RunTestsView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── get_metric_definition → metric definition ─────────────────────────────
  if (item.toolName === "get_metric_definition") {
    return (
      <Panel>
        <PanelHeader
          title='Metric Definition'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <GetMetricDefinitionView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── get_join_path → join path ─────────────────────────────────────────────
  if (item.toolName === "get_join_path") {
    return (
      <Panel>
        <PanelHeader
          title='Join Path'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <GetJoinPathView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── sample_column / sample_columns → column explorer ─────────────────────
  if (item.toolName === "sample_column" || item.toolName === "sample_columns") {
    return (
      <Panel>
        <PanelHeader
          title='Column Sample'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <SampleColumnView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "get_column_values") {
    return (
      <Panel>
        <PanelHeader
          title='Column Values'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ColumnValuesView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "get_column_range") {
    return (
      <Panel>
        <PanelHeader
          title='Column Range'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ColumnRangeView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  if (item.toolName === "count_rows") {
    return (
      <Panel>
        <PanelHeader
          title='Count Rows'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <CountRowsView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── compile_semantic_query → airlayer compile result ─────────────────────
  if (item.toolName === "compile_semantic_query") {
    return (
      <Panel>
        <PanelHeader
          title='Compile Semantic Query'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <CompileSemanticQueryView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── resolve_schema → schema tables ───────────────────────────────────────
  if (item.toolName === "resolve_schema") {
    return (
      <Panel>
        <PanelHeader
          title='Schema'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ResolveSchemaView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── Procedure step → status view ─────────────────────────────────────────
  // toolInput is the literal string "Running…" for procedure steps, never JSON.
  if (item.toolInput === "Running\u2026") {
    return (
      <Panel>
        <PanelHeader
          title={item.toolName}
          subtitle={
            item.isStreaming ? "Running" : item.toolOutput === "Completed" ? "Completed" : "Failed"
          }
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <ProcedureStepView item={item} />
        </PanelContent>
      </Panel>
    );
  }

  // ── Generic fallback ──────────────────────────────────────────────────────
  return (
    <Panel>
      <PanelHeader
        title={item.toolName}
        subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
        onClose={onClose}
      />
      <PanelContent scrollable={false} padding={false} className='min-h-0'>
        <RawArtifactView item={item} />
      </PanelContent>
    </Panel>
  );
};

export default AnalyticsArtifactSidebar;
