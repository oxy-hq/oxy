import { Editor } from "@monaco-editor/react";
import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { useState } from "react";
import AppPreview from "@/components/AppPreview";
import SqlArtifactPanel from "@/components/ArtifactPanel/ArtifactsContent/sql";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import type { ArtifactItem, SqlItem } from "@/hooks/analyticsSteps";
import type { SseEvent } from "@/hooks/useAppBuilderRun";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FileService } from "@/services/api/files";
import {
  ColumnRangeView,
  ColumnValuesView,
  CountRowsView,
  GetJoinPathView,
  RawArtifactView,
  ResolveSchemaView,
  SearchCatalogView
} from "../analytics/AnalyticsArtifactViews";
import {
  sqlArtifactFromExecutePreview,
  sqlArtifactFromPreviewData,
  sqlArtifactFromSqlItem
} from "../analytics/analyticsArtifactHelpers";

export type AppPreviewItem = { kind: "app_preview"; appPath64: string };
export type AppBuilderSelectableItem = ArtifactItem | SqlItem | AppPreviewItem;

interface Props {
  item: AppBuilderSelectableItem;
  runEvents?: SseEvent[];
  isRunning?: boolean;
  onClose: () => void;
}

const AppPreviewPanel = ({ appPath64, onClose }: { appPath64: string; onClose: () => void }) => {
  const [activeTab, setActiveTab] = useState<"preview" | "code">("preview");
  const { project, branchName } = useCurrentProjectBranch();

  const { data: yamlContent, isLoading: isLoadingYaml } = useQuery({
    queryKey: ["app-yaml", project.id, branchName, appPath64],
    queryFn: () => FileService.getFile(project.id, appPath64, branchName),
    enabled: activeTab === "code"
  });

  return (
    <Panel>
      <PanelHeader
        title={
          <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as "preview" | "code")}>
            <TabsList variant='line' className='h-auto'>
              <TabsTrigger value='preview'>Preview</TabsTrigger>
              <TabsTrigger value='code'>Code</TabsTrigger>
            </TabsList>
          </Tabs>
        }
        onClose={onClose}
      />
      <PanelContent scrollable={false} padding={false} className='flex-1 overflow-hidden'>
        {activeTab === "preview" ? (
          <AppPreview appPath64={appPath64} runButton={false} />
        ) : (
          <div className='h-full'>
            {isLoadingYaml ? (
              <div className='flex h-full items-center justify-center'>
                <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
              </div>
            ) : (
              <Editor
                height='100%'
                width='100%'
                theme='vs-dark'
                language='yaml'
                value={yamlContent ?? ""}
                options={{
                  readOnly: true,
                  scrollBeyondLastLine: false,
                  automaticLayout: true,
                  minimap: { enabled: false },
                  fontSize: 13
                }}
              />
            )}
          </div>
        )}
      </PanelContent>
    </Panel>
  );
};

const AppBuilderArtifactSidebar = ({ item, onClose }: Props) => {
  // ── kind === "app_preview" ────────────────────────────────────────────────
  if (item.kind === "app_preview") {
    return <AppPreviewPanel appPath64={item.appPath64} onClose={onClose} />;
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

  // ── preview_data → SQL panel ──────────────────────────────────────────────
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

  // ── get_column_values → values list ──────────────────────────────────────
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

  // ── get_column_range → range stats ───────────────────────────────────────
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

  // ── get_join_path → join expression ──────────────────────────────────────
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

  // ── count_rows → row count ────────────────────────────────────────────────
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

export default AppBuilderArtifactSidebar;
