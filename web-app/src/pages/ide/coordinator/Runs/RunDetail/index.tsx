import { Info } from "lucide-react";
import type React from "react";
import { Link, useParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import useRunTree from "@/hooks/api/coordinator/useRunTree";
import { JOB_TYPE, sourceTypeToJobType } from "../../components/constants";
import { EmptyState, ErrorState, LoadingState } from "../../components/PageState";
import { useCoordinatorRoutes } from "../../components/useCoordinatorRoutes";
import { AgentEventLog } from "./components/AgentEventLog";
import { Conversation } from "./components/Conversation";
import { EltBody } from "./components/Elt";
import { LlmUsageCard } from "./components/LlmUsageCard";
import { RunHeader } from "./components/RunHeader";
import { TaskTree } from "./components/TaskTree";
import { Waterfall } from "./components/Waterfall";
import { WorkflowBody } from "./components/Workflow";

/** What each job type still needs from the coordinator metrics backend. */
const TYPE_FOLLOWUP: Record<string, string> = {
  agent:
    "Waterfall + conversation views are wired; sub-agent fan-out and inline thinking previews land next.",
  dag: "Graph + step inspector are live; sub-procedure steps embed their nested trace. Real `depends_on` edges (vs. execution-order chain) land when the YAML graph is parsed server-side.",
  elt: "Lineage header, tri-phase (extract/normalize/load) bars per table, and schema-evolution banner are live. Throughput (rows/sec, bytes) lands when the airway events carry byte counts."
};

/**
 * Run detail — one execution. The header is identical for every job type;
 * the body is polymorphic. Agent runs get the waterfall/conversation tabs;
 * workflow and airway runs keep the task-tree view that matches their
 * own DAG-shaped execution model.
 */
const RunDetailPage: React.FC = () => {
  const { runId } = useParams<{ runId: string }>();
  const { data, isPending, error, refetch } = useRunTree(runId);
  const routes = useCoordinatorRoutes();

  if (isPending) return <LoadingState />;
  if (error) return <ErrorState message='Failed to load run' onRetry={refetch} />;

  const root = data?.nodes.find((n) => n.run_id === data.root_id);
  if (!data || !root) {
    return (
      <EmptyState
        title='Run not found'
        hint='This run may have been pruned from history.'
        action={
          <Button asChild size='sm' variant='outline'>
            <Link to={routes.RUNS}>Back to runs</Link>
          </Button>
        }
      />
    );
  }

  const jobType = sourceTypeToJobType(root.source_type);
  const isAgent = jobType === "agent";
  const isWorkflow = root.source_type === "workflow";
  const isAirway = root.source_type === "airway";

  return (
    <div className='flex h-full flex-col'>
      <RunHeader root={root} jobType={jobType} nodeCount={data.nodes.length} />
      <div className='flex-1 overflow-y-auto'>
        {(root.llm_usage || isAgent) && (
          <LlmUsageCard usage={root.llm_usage} events={isAgent ? root.event_log : undefined} />
        )}
        <div className='flex items-start gap-2 border-border border-b bg-muted/30 px-4 py-2 text-muted-foreground text-xs'>
          <Info className='mt-0.5 h-3.5 w-3.5 shrink-0' />
          <span>
            <span className='font-medium text-foreground'>{JOB_TYPE[jobType].label}</span> —
            debugging unit is the {JOB_TYPE[jobType].unit}. {TYPE_FOLLOWUP[jobType]}
          </span>
        </div>
        {isAgent ? (
          <AgentRunBody
            events={root.event_log ?? []}
            question={root.question}
            answer={root.answer}
            errorMessage={root.error_message}
          />
        ) : isWorkflow ? (
          <WorkflowBody steps={root.dag_steps ?? []} events={root.event_log ?? []} />
        ) : isAirway ? (
          <EltBody
            tables={root.elt_tables ?? []}
            events={root.event_log ?? []}
            pipelineName={root.pipeline_name}
            sourceKind={root.source_kind}
            destinationLabel={root.destination_label}
            runError={root.error_message}
          />
        ) : (
          <TaskTree nodes={data.nodes} rootId={data.root_id} />
        )}
      </div>
    </div>
  );
};

const AgentRunBody: React.FC<{
  events: import("@/services/api/coordinator").RunEventEntry[];
  question: string;
  answer?: string;
  errorMessage?: string;
}> = ({ events, question, answer, errorMessage }) => (
  <Tabs defaultValue='waterfall' className='gap-0'>
    <TabsList className='mx-3 mt-3'>
      <TabsTrigger value='waterfall'>Waterfall</TabsTrigger>
      <TabsTrigger value='conversation'>Conversation</TabsTrigger>
      <TabsTrigger value='events'>Events</TabsTrigger>
    </TabsList>
    <TabsContent value='waterfall' className='mt-0'>
      <Waterfall events={events} />
    </TabsContent>
    <TabsContent value='conversation' className='mt-0'>
      <Conversation
        events={events}
        question={question}
        answer={answer}
        errorMessage={errorMessage}
      />
    </TabsContent>
    <TabsContent value='events' className='mt-0'>
      <AgentEventLog events={events} />
    </TabsContent>
  </Tabs>
);

export default RunDetailPage;
