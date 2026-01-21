import {
  Workflow,
  Bot,
  Folder,
  Globe,
  Rocket,
  ClipboardList,
  Play,
  CheckCircle,
  Target,
  Search,
  Database,
  Plug,
  RotateCw,
  FileText,
  BarChart3,
  Zap,
  GitBranch,
  RefreshCw,
  Settings,
  Download,
  Hash,
  Activity,
  Wrench,
} from "lucide-react";

interface SpanIconProps {
  spanName: string;
  className?: string;
}

/**
 * Renders the appropriate Lucide icon for a span type
 */
// eslint-disable-next-line sonarjs/cognitive-complexity
export function SpanIcon({ spanName, className = "h-4 w-4" }: SpanIconProps) {
  // LLM calls
  if (spanName.startsWith("llm.")) {
    return <Bot className={className} />;
  }

  // Tool execution
  if (spanName === "sql.execute") {
    return <Database className={className} />;
  }
  if (spanName === "validate_sql.execute") {
    return <CheckCircle className={className} />;
  }
  if (spanName === "workflow.execute") {
    return <Workflow className={className} />;
  }
  if (spanName === "retrieval.execute") {
    return <Search className={className} />;
  }
  if (spanName === "omni_query.execute") {
    return <Plug className={className} />;
  }
  if (spanName === "visualize.execute") {
    return <BarChart3 className={className} />;
  }
  if (spanName === "create_data_app.execute") {
    return <FileText className={className} />;
  }
  if (spanName === "tool_launcher.execute") {
    return <Rocket className={className} />;
  }
  if (spanName === "semantic_query.execute") {
    return <Search className={className} />;
  }
  if (spanName === "tool_call.execute") {
    return <Zap className={className} />;
  }
  if (spanName.startsWith("tool.")) {
    return <Wrench className={className} />;
  }

  // Workflow launcher operations
  if (spanName === "workflow.launcher.with_project") {
    return <Folder className={className} />;
  }
  if (spanName === "workflow.launcher.get_global_context") {
    return <Globe className={className} />;
  }
  if (spanName === "workflow.launcher.launch") {
    return <Rocket className={className} />;
  }
  if (spanName.startsWith("workflow.launcher.")) {
    return <ClipboardList className={className} />;
  }

  // Workflow run
  if (spanName === "workflow.run_workflow") {
    return <Workflow className={className} />;
  }

  // Workflow task types
  if (spanName === "workflow.task.execute") {
    return <CheckCircle className={className} />;
  }
  if (spanName.startsWith("workflow.task.agent.")) {
    return <Target className={className} />;
  }
  if (spanName.startsWith("workflow.task.semantic_query.")) {
    return <Search className={className} />;
  }
  if (spanName.startsWith("workflow.task.execute_sql.")) {
    return <Database className={className} />;
  }
  if (spanName.startsWith("workflow.task.omni_query.")) {
    return <Plug className={className} />;
  }
  if (spanName.startsWith("workflow.task.loop.")) {
    return <RotateCw className={className} />;
  }
  if (spanName.startsWith("workflow.task.formatter.")) {
    return <FileText className={className} />;
  }
  if (spanName.startsWith("workflow.task.sub_workflow.")) {
    return <BarChart3 className={className} />;
  }
  if (spanName.startsWith("workflow.task.")) {
    return <ClipboardList className={className} />;
  }

  // General workflow operations
  if (spanName.startsWith("workflow.")) {
    return <Workflow className={className} />;
  }

  // Agent operations - specific agents first
  if (spanName.includes("routing_agent")) {
    return <GitBranch className={className} />;
  }
  if (spanName.includes("fallback_agent")) {
    return <RefreshCw className={className} />;
  }
  if (spanName.includes("default_agent")) {
    return <Target className={className} />;
  }
  if (spanName === "agent.run_agent") {
    return <Bot className={className} />;
  }
  if (spanName === "agent.get_global_context") {
    return <Globe className={className} />;
  }
  if (spanName === "agent.load_config") {
    return <Settings className={className} />;
  }
  if (spanName === "agent.execute") {
    return <Play className={className} />;
  }
  if (spanName.startsWith("agent.")) {
    return <Bot className={className} />;
  }

  // Data operations
  if (spanName === "load") {
    return <Download className={className} />;
  }
  if (spanName === "count_rows") {
    return <Hash className={className} />;
  }

  return <Activity className={className} />;
}
