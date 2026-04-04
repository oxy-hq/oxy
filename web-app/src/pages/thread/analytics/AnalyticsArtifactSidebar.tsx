import SqlArtifactPanel from "@/components/ArtifactPanel/ArtifactsContent/sql";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import type { ArtifactItem, ProcedureItem, SqlItem } from "@/hooks/analyticsSteps";
import type { AnalyticsDisplayBlock, SseEvent } from "@/hooks/useAnalyticsRun";
import { extractDisplayBlockForSeq } from "@/hooks/useAnalyticsRun";
import ProcedureRunDagPanel from "../agentic/ProcedureRunDagPanel";
import {
  AskUserView,
  ChartSection,
  CompileSemanticQueryView,
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
import { sqlArtifactFromExecutePreview, sqlArtifactFromSqlItem } from "./analyticsArtifactHelpers";

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
    return (
      <Panel>
        <PanelHeader
          title='SQL Query'
          subtitle={
            item.rowCount !== undefined
              ? `${item.rowCount} rows · ${item.durationMs ?? 0}ms`
              : undefined
          }
          onClose={onClose}
        />
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

  // ── render_chart → config + rendered chart ───────────────────────────────
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

  // ── ask_user → question + user response ─────────────────────────────────
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

  // ── sample_columns → column explorer ─────────────────────────────────────
  if (item.toolName === "sample_columns") {
    return (
      <Panel>
        <PanelHeader
          title='Column Samples'
          subtitle={item.durationMs !== undefined ? `${item.durationMs}ms` : undefined}
          onClose={onClose}
        />
        <PanelContent scrollable={false} padding={false} className='min-h-0'>
          <SampleColumnView item={item} />
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
