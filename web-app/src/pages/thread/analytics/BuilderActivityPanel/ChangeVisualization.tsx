import type { BuilderProposedChange } from "@/hooks/useBuilderActivity";
import { AgentGraph } from "./AgentGraph";
import { AwGraph } from "./AwGraph";
import { DataAppGraph } from "./DataAppGraph";
import { GenericFileDiff } from "./GenericFileDiff";
import { SemanticTopicGraph } from "./SemanticTopicGraph";
import { SemanticViewGraph } from "./SemanticViewGraph";
import { TestGraph } from "./TestGraph";
import {
  tryParseAgent,
  tryParseApp,
  tryParseAw,
  tryParseTest,
  tryParseTopic,
  tryParseView,
  tryParseWorkflow
} from "./types";
import { WorkflowGraph } from "./WorkflowGraph";

export interface ChangeVisualizationProps {
  change: BuilderProposedChange;
}

const ChangeVisualization = ({ change }: ChangeVisualizationProps) => {
  const p = change.filePath;

  const isViewFile = p.endsWith(".view.yml") || p.endsWith(".view.yaml");
  const isAppFile = p.endsWith(".app.yml") || p.endsWith(".app.yaml");
  const isWorkflowFile =
    p.endsWith(".workflow.yml") ||
    p.endsWith(".workflow.yaml") ||
    p.endsWith(".procedure.yml") ||
    p.endsWith(".procedure.yaml") ||
    p.endsWith(".automation.yml") ||
    p.endsWith(".automation.yaml");
  const isAgentFile = p.endsWith(".agent.yml") || p.endsWith(".agent.yaml");
  const isTopicFile = p.endsWith(".topic.yml") || p.endsWith(".topic.yaml");
  const isAwFile =
    p.endsWith(".aw.yml") ||
    p.endsWith(".aw.yaml") ||
    p.endsWith(".agentic.yml") ||
    p.endsWith(".agentic.yaml");
  const isTestFile = p.endsWith(".test.yml") || p.endsWith(".test.yaml");

  const old = change.oldContent || null;

  const newView = isViewFile ? tryParseView(change.newContent) : null;
  const oldView = isViewFile ? tryParseView(old ?? "") : null;

  const newApp = isAppFile ? tryParseApp(change.newContent) : null;
  const oldApp = isAppFile ? tryParseApp(old ?? "") : null;

  const newWf = isWorkflowFile ? tryParseWorkflow(change.newContent) : null;
  const oldWf = isWorkflowFile ? tryParseWorkflow(old ?? "") : null;

  const newAgent = isAgentFile ? tryParseAgent(change.newContent) : null;
  const oldAgent = isAgentFile ? tryParseAgent(old ?? "") : null;

  const newTopic = isTopicFile ? tryParseTopic(change.newContent) : null;
  const oldTopic = isTopicFile ? tryParseTopic(old ?? "") : null;

  const newAw = isAwFile ? tryParseAw(change.newContent) : null;
  const oldAw = isAwFile ? tryParseAw(old ?? "") : null;

  const newTest = isTestFile ? tryParseTest(change.newContent) : null;
  const oldTest = isTestFile ? tryParseTest(old ?? "") : null;

  if (newView) return <SemanticViewGraph change={change} oldView={oldView} newView={newView} />;
  if (newApp) return <DataAppGraph change={change} oldApp={oldApp} newApp={newApp} />;
  if (newWf) return <WorkflowGraph change={change} oldWf={oldWf} newWf={newWf} />;
  if (newAgent) return <AgentGraph change={change} oldAgent={oldAgent} newAgent={newAgent} />;
  if (newTopic)
    return <SemanticTopicGraph change={change} oldTopic={oldTopic} newTopic={newTopic} />;
  if (newAw) return <AwGraph change={change} oldAw={oldAw} newAw={newAw} />;
  if (newTest) return <TestGraph change={change} oldTest={oldTest} newTest={newTest} />;

  return <GenericFileDiff change={change} />;
};

export default ChangeVisualization;
