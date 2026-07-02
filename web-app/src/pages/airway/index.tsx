/**
 * Airway pipelines UI — two surfaces (Dagster/Prefect/Airbyte shape):
 *
 * - {@link AirwayPipelinePage} (`pipelines/:pathb64`) — the landing
 *   page, never empty: **Overview** (description + lineage + recent
 *   runs — built out in a follow-up) and **Runs** (run list). Running
 *   the pipeline opens its run detail.
 * - {@link AirwayRunDetailPage} (`pipelines/:pathb64/runs/:runId`) —
 *   a single run: live Lineage / Grid + raw event trace.
 *
 * Both are prop-driven so they work standalone (routed, default
 * {@link AirwayPage}) and embedded in the IDE Pipelines section
 * (master-detail via local state — callbacks instead of navigation).
 */

import { AlertTriangle, ChevronRight, Database, Loader2, PlayIcon, StopCircle } from "lucide-react";
import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import BackfillAirwayModal from "@/components/airway/BackfillAirwayModal";
import BackfillRangesPanel from "@/components/airway/BackfillRangesPanel";
import LineageGraph from "@/components/airway/LineageGraph";
import PhaseBar from "@/components/airway/PhaseBar";
import PipelineOverview from "@/components/airway/PipelineOverview";
import QuickBooksReconnect from "@/components/airway/QuickBooksReconnect";
import ResourceGrid from "@/components/airway/ResourceGrid";
import RetryFailedTablesButton from "@/components/airway/RetryFailedTablesButton";
import RunHistory from "@/components/airway/RunHistory";
import RunTimeline from "@/components/airway/RunTimeline";
import PageHeader from "@/components/PageHeader";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { useAirwayRunController } from "@/hooks/api/airway/useAirway";
import { decodeBase64 } from "@/libs/encoding";
import { cn } from "@/libs/shadcn/utils";
import type { AirwayRunStatus } from "@/utils/airwayReducer";

const RUN_STATUS_VARIANT: Record<
  AirwayRunStatus,
  "default" | "secondary" | "destructive" | "outline"
> = {
  running: "secondary",
  done: "default",
  completed_with_errors: "outline",
  failed: "destructive",
  cancelled: "outline"
};

const RUN_STATUS_LABEL: Record<AirwayRunStatus, string> = {
  running: "running",
  done: "done",
  completed_with_errors: "completed with errors",
  failed: "failed",
  cancelled: "cancelled"
};

/** Slim embedded bar / full PageHeader, shared by both pages. */
const Chrome: React.FC<{
  hideHeader: boolean;
  pipelineRef: string;
  badge?: React.ReactNode;
  children: React.ReactNode;
}> = ({ hideHeader, pipelineRef, badge, children }) =>
  hideHeader ? (
    <div className='flex items-center gap-2 border-border border-b-1 px-4 py-2'>
      {badge}
      <div className='ml-auto'>{children}</div>
    </div>
  ) : (
    <PageHeader className='items-center gap-2 border-border border-b-1'>
      <div className='hidden flex-1 md:block' />
      <div className='flex min-w-0 flex-1 items-center justify-center gap-1'>
        <Database className='h-4 w-4 shrink-0' />
        <span className='min-w-0 truncate text-sm'>{pipelineRef}</span>
        {badge && <span className='ml-2'>{badge}</span>}
      </div>
      <div className='flex flex-1 justify-end'>{children}</div>
    </PageHeader>
  );

/** Pull the backend's plain-text reason out of a failed start. */
function startErrorMessage(err: unknown): string {
  const data = (err as { response?: { data?: unknown } })?.response?.data;
  if (typeof data === "string" && data.trim()) return data;
  if (err instanceof Error && err.message) return err.message;
  return "Failed to start the pipeline run.";
}

/* ───────────────────────── Pipeline page ───────────────────────── */

export const AirwayPipelinePage: React.FC<{
  pathb64: string;
  hideHeader?: boolean;
  /** Embedded master-detail opens the run via local state; standalone
   *  navigates. */
  onOpenRun?: (runId: string) => void;
}> = ({ pathb64, hideHeader = false, onOpenRun }) => {
  const pipelineRef = useMemo(() => decodeBase64(pathb64), [pathb64]);
  const ctrl = useAirwayRunController();
  const navigate = useNavigate();
  // Controlled so a started chunked backfill can jump straight to Coverage.
  const [tab, setTab] = useState("overview");

  const openRun = (runId: string) => {
    if (onOpenRun) onOpenRun(runId);
    else navigate(`runs/${runId}`, { relative: "path" });
  };

  const run = async () => {
    try {
      const runId = await ctrl.launch({ pipeline_ref: pipelineRef });
      openRun(runId);
    } catch (e) {
      // Submit-time failures (bad pipeline_ref, spec/config parse,
      // validation) return 400 with no run row — there's nothing to
      // navigate to, so surface the reason instead of silently
      // leaving the user on an empty page.
      toast.error(startErrorMessage(e));
    }
  };

  return (
    <div className='flex h-full flex-col'>
      <Chrome hideHeader={hideHeader} pipelineRef={pipelineRef}>
        <BackfillAirwayModal
          pipelineRef={pipelineRef}
          onStarted={openRun}
          onChunkedStarted={() => setTab("coverage")}
        />
        <Button size='sm' onClick={run} disabled={ctrl.starting} aria-label='Run this pipeline'>
          {ctrl.starting ? (
            <Loader2 className='h-4 w-4 animate-spin' />
          ) : (
            <PlayIcon className='h-4 w-4' />
          )}
          Run
        </Button>
      </Chrome>

      <Tabs value={tab} onValueChange={setTab} className='flex min-h-0 flex-1 flex-col'>
        <TabsList className='mx-4 mt-2'>
          <TabsTrigger value='overview'>Overview</TabsTrigger>
          <TabsTrigger value='runs'>Runs</TabsTrigger>
          <TabsTrigger value='coverage'>Coverage</TabsTrigger>
        </TabsList>
        <TabsContent value='overview' className='min-h-0 flex-1 overflow-auto'>
          <PipelineOverview pipelineRef={pipelineRef} onOpenRun={openRun} />
        </TabsContent>
        <TabsContent value='runs' className='min-h-0 flex-1 overflow-auto'>
          <RunHistory pipelineRef={pipelineRef} onSelect={openRun} />
        </TabsContent>
        <TabsContent value='coverage' className='min-h-0 flex-1 overflow-auto'>
          <BackfillRangesPanel pipelineRef={pipelineRef} />
        </TabsContent>
      </Tabs>
    </div>
  );
};

/* ──────────────────────── Run detail page ──────────────────────── */

export const AirwayRunDetailPage: React.FC<{
  pathb64: string;
  runId: string;
  hideHeader?: boolean;
  /** Back to the pipeline page. Standalone navigates if omitted. */
  onBack?: () => void;
  /** Open another run (e.g. "Run again"). Standalone navigates. */
  onOpenRun?: (runId: string) => void;
}> = ({ pathb64, runId, hideHeader = false, onBack, onOpenRun }) => {
  const pipelineRef = useMemo(() => decodeBase64(pathb64), [pathb64]);
  const ctrl = useAirwayRunController();
  const { view, events, runId: activeRunId, streaming, starting, stopping } = ctrl;
  const navigate = useNavigate();

  // Adopt the URL/prop run id (and re-adopt if it changes).
  useEffect(() => {
    if (runId && runId !== activeRunId) ctrl.setRunId(runId);
  }, [runId, activeRunId, ctrl]);

  const back = () => {
    if (onBack) onBack();
    else navigate("../..", { relative: "path" });
  };

  const isRunning = view.status === "running" && streaming;

  const controls = isRunning ? (
    <Button
      size='sm'
      variant='outline'
      onClick={() => ctrl.stop()}
      disabled={stopping}
      aria-label='Cancel the running pipeline'
    >
      {stopping ? <Loader2 className='h-4 w-4 animate-spin' /> : <StopCircle className='h-4 w-4' />}
      Cancel
    </Button>
  ) : (
    <Button
      size='sm'
      onClick={async () => {
        try {
          const id = await ctrl.launch({ pipeline_ref: pipelineRef });
          if (onOpenRun) onOpenRun(id);
          else navigate(`../${id}`, { relative: "path" });
        } catch (e) {
          toast.error(startErrorMessage(e));
        }
      }}
      disabled={starting}
      aria-label='Run this pipeline again'
    >
      {starting ? <Loader2 className='h-4 w-4 animate-spin' /> : <PlayIcon className='h-4 w-4' />}
      Run again
    </Button>
  );

  const badge = (
    <Badge variant={RUN_STATUS_VARIANT[view.status]}>{RUN_STATUS_LABEL[view.status]}</Badge>
  );

  return (
    <div className='flex h-full flex-col'>
      <Chrome hideHeader={hideHeader} pipelineRef={pipelineRef} badge={badge}>
        <Button size='sm' variant='ghost' onClick={back} aria-label='Back to pipeline'>
          ← Pipeline
        </Button>
        {controls}
      </Chrome>

      <div className='flex flex-1 flex-col overflow-auto'>
        <PhaseBar phase={view.phase} loadId={view.loadId} />

        {view.schemaChanges != null && (
          <Collapsible className='mx-4 mb-2 rounded-md border border-border bg-muted/40'>
            <CollapsibleTrigger className='group flex w-full items-center gap-2 px-3 py-2 text-sm'>
              <ChevronRight className='h-3 w-3 transition-transform group-data-[state=open]:rotate-90' />
              <span className='font-medium'>Schema evolved</span>
              <span className='text-muted-foreground'>— new tables/columns appeared this run</span>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <pre className='max-h-48 overflow-auto border-border border-t px-3 py-2 font-mono text-xs'>
                {JSON.stringify(view.schemaChanges, null, 2)}
              </pre>
            </CollapsibleContent>
          </Collapsible>
        )}

        {view.status === "failed" && view.error && (
          <div
            role='alert'
            className='mx-4 mb-2 flex flex-col gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-destructive text-sm'
          >
            <div className='flex items-start gap-2'>
              <AlertTriangle className='mt-0.5 h-4 w-4 shrink-0' />
              <span className='break-words'>{view.error}</span>
            </div>
            {/* For a quickbooks pipeline, offer one-click re-auth — the
                common cause of a failed run is an expired refresh token. */}
            <div className='pl-6'>
              <QuickBooksReconnect pipelineRef={pipelineRef} />
            </div>
          </div>
        )}

        {view.failedResources.length > 0 && (
          <div className='mx-4 mb-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm'>
            <div className='flex items-center gap-2 font-medium'>
              <AlertTriangle className='h-4 w-4 shrink-0' />
              Completed with {view.failedResources.length} skipped resource
              {view.failedResources.length === 1 ? "" : "s"}
              <div className='ml-auto'>
                <RetryFailedTablesButton
                  failedTables={view.failedResources.map((f) => f.table)}
                  pending={starting}
                  onConfirm={async (tables) => {
                    try {
                      const id = await ctrl.launch({
                        pipeline_ref: pipelineRef,
                        resources: tables
                      });
                      if (onOpenRun) onOpenRun(id);
                      else navigate(`../${id}`, { relative: "path" });
                    } catch (e) {
                      toast.error(startErrorMessage(e));
                    }
                  }}
                />
              </div>
            </div>
            <ul className='mt-1 space-y-0.5 text-muted-foreground text-xs'>
              {view.failedResources.map((f) => (
                <li key={f.table} className='break-words'>
                  <span className='font-mono'>{f.table}</span> — {f.error}
                </li>
              ))}
            </ul>
          </div>
        )}

        <Tabs defaultValue='graph' className='flex flex-1 flex-col border-border border-t'>
          <TabsList className='mx-4 mt-2'>
            <TabsTrigger value='graph'>Lineage</TabsTrigger>
            <TabsTrigger value='grid'>Grid</TabsTrigger>
            <TabsTrigger value='timeline'>Timeline</TabsTrigger>
          </TabsList>
          <TabsContent value='graph'>
            <LineageGraph view={view} />
          </TabsContent>
          <TabsContent value='grid'>
            <ResourceGrid resources={view.resources} />
          </TabsContent>
          <TabsContent value='timeline'>
            <RunTimeline view={view} />
          </TabsContent>
        </Tabs>

        <div className='mt-auto'>
          <Collapsible className='border-border border-t'>
            <CollapsibleTrigger className='group flex w-full items-center gap-2 px-4 py-2 text-muted-foreground text-xs hover:bg-muted/50'>
              <ChevronRight className='h-3 w-3 transition-transform group-data-[state=open]:rotate-90' />
              Raw event trace ({events.length})
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className='max-h-64 overflow-auto bg-muted/30 px-4 py-2 font-mono text-xs'>
                {events.length === 0 ? (
                  <div className='text-muted-foreground'>No events yet.</div>
                ) : (
                  events.map((e, i) => (
                    <div
                      // biome-ignore lint/suspicious/noArrayIndexKey: append-only event log — never reordered or filtered, so the index is a stable identity
                      key={i}
                      className={cn(
                        "whitespace-pre-wrap break-all py-0.5",
                        e.type === "pipeline_error" && "text-destructive"
                      )}
                    >
                      <span className='text-muted-foreground'>{e.type}</span>{" "}
                      {JSON.stringify(e.payload)}
                    </div>
                  ))
                )}
              </div>
            </CollapsibleContent>
          </Collapsible>
        </div>
      </div>
    </div>
  );
};

/* ─────────────────── Standalone route entry point ─────────────────── */

const AirwayPage: React.FC = () => {
  const { pathb64, runId } = useParams();
  const pb = pathb64 ?? "";
  return runId ? (
    <AirwayRunDetailPage key={`${pb}:${runId}`} pathb64={pb} runId={runId} />
  ) : (
    <AirwayPipelinePage key={pb} pathb64={pb} />
  );
};

export default AirwayPage;
