import type { WaterfallSpan } from "@/services/api/traces";
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
  type LucideIcon,
} from "lucide-react";

/**
 * Formats a span name into a user-friendly label
 */
export function formatSpanLabel(spanName: string): string {
  // Map of specific span names to friendly labels
  const labelMap: Record<string, string> = {
    // Workflow launcher
    "workflow.launcher.with_project": "Load Project",
    "workflow.launcher.get_global_context": "Get Global Context",
    "workflow.launcher.launch": "Launch Workflow",

    // Workflow run
    "workflow.run_workflow": "Workflow",

    // Workflow task execution
    "workflow.task.execute": "Execute Task",
    "workflow.task.agent.execute": "Execute Agent Task",

    // Semantic query operations
    "workflow.task.semantic_query.render": "Render Semantic Query",
    "workflow.task.semantic_query.map": "Map Semantic Query",
    "workflow.task.semantic_query.compile": "Compile Query to SQL",
    "workflow.task.semantic_query.execute": "Execute Semantic Query",
    "workflow.task.semantic_query.get_sql_from_cubejs":
      "Generate SQL from CubeJS",
    "workflow.task.semantic_query.execute_sql": "Execute SQL Query",

    // SQL execution operations
    "workflow.task.execute_sql.map": "Map SQL Task",
    "workflow.task.execute_sql.execute": "Execute SQL",

    // Omni query operations
    "workflow.task.omni_query.map": "Map Omni Query",
    "workflow.task.omni_query.execute": "Execute Omni Query",
    "workflow.task.omni_query.execute_query": "Run Omni Query",

    // Loop operations
    "workflow.task.loop.map": "Map Loop",
    "workflow.task.loop.item_map": "Process Loop Item",

    // Formatter
    "workflow.task.formatter.execute": "Format Output",

    // Sub-workflow
    "workflow.task.sub_workflow.execute": "Execute Sub-Workflow",

    // Agent operations
    "agent.run_agent": "Agent",
    "agent.get_global_context": "Get Global Context",
    "agent.load_config": "Load Agent Config",
    "agent.execute": "Execute Agent",

    // Tool operations
    "tool_call.execute": "Execute Tool Call",

    // Data operations
    load: "Load Data",
    count_rows: "Count Rows",
  };

  // Check if we have a specific label mapping
  if (labelMap[spanName]) {
    return labelMap[spanName];
  }

  // Handle pattern-based formatting for unmapped spans
  // Replace dots with spaces and capitalize words
  return spanName
    .split(".")
    .map((part) => {
      // Handle snake_case by replacing underscores with spaces
      return part
        .split("_")
        .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
        .join(" ");
    })
    .join(" › ");
}

/**
 * Gets the Lucide icon component for a span
 */
// eslint-disable-next-line sonarjs/cognitive-complexity
export function getSpanIcon(spanName: string): LucideIcon {
  // LLM calls
  if (spanName.startsWith("llm.")) return Bot;

  // Tool execution
  if (spanName === "tool_call.execute") return Zap;
  if (spanName.startsWith("tool.")) return Wrench;

  // Workflow launcher operations
  if (spanName === "workflow.launcher.with_project") return Folder;
  if (spanName === "workflow.launcher.get_global_context") return Globe;
  if (spanName === "workflow.launcher.launch") return Rocket;
  if (spanName.startsWith("workflow.launcher.")) return ClipboardList;

  // Workflow run
  if (spanName === "workflow.run_workflow") return Play;

  // Workflow task types
  if (spanName === "workflow.task.execute") return CheckCircle;
  if (spanName.startsWith("workflow.task.agent.")) return Target;
  if (spanName.startsWith("workflow.task.semantic_query.")) return Search;
  if (spanName.startsWith("workflow.task.execute_sql.")) return Database;
  if (spanName.startsWith("workflow.task.omni_query.")) return Plug;
  if (spanName.startsWith("workflow.task.loop.")) return RotateCw;
  if (spanName.startsWith("workflow.task.formatter.")) return FileText;
  if (spanName.startsWith("workflow.task.sub_workflow.")) return BarChart3;
  if (spanName.startsWith("workflow.task.")) return ClipboardList;

  // General workflow operations
  if (spanName.startsWith("workflow.")) return Workflow;

  // Agent operations - specific agents first
  if (spanName.includes("routing_agent")) return GitBranch;
  if (spanName.includes("fallback_agent")) return RefreshCw;
  if (spanName.includes("default_agent")) return Target;
  if (spanName === "agent.run_agent") return Rocket;
  if (spanName === "agent.get_global_context") return Globe;
  if (spanName === "agent.load_config") return Settings;
  if (spanName === "agent.execute") return Play;
  if (spanName.startsWith("agent.")) return Bot;

  // Data operations
  if (spanName === "load") return Download;
  if (spanName === "count_rows") return Hash;

  return Activity;
}

// eslint-disable-next-line sonarjs/cognitive-complexity
export function getSpanColor(span: WaterfallSpan): string {
  if (span.statusCode === "ERROR") return "bg-destructive";
  const spanName = span.spanName;

  // LLM calls - purple
  if (spanName.startsWith("llm.")) return "bg-purple-500";

  // Tool execution - amber/orange
  if (spanName === "tool_call.execute") return "bg-orange-500";
  if (spanName.startsWith("tool.")) return "bg-amber-500";

  // Workflow launcher - emerald/green shades
  if (spanName === "workflow.launcher.with_project") return "bg-emerald-600";
  if (spanName === "workflow.launcher.get_global_context")
    return "bg-emerald-500";
  if (spanName === "workflow.launcher.launch") return "bg-emerald-700";
  if (spanName.startsWith("workflow.launcher.")) return "bg-emerald-500";

  // Workflow run - vibrant green
  if (spanName === "workflow.run_workflow") return "bg-green-600";

  // Workflow task types - various greens and teals
  if (spanName === "workflow.task.execute") return "bg-green-500";
  if (spanName.startsWith("workflow.task.agent.")) return "bg-teal-600";
  if (spanName.startsWith("workflow.task.semantic_query."))
    return "bg-cyan-600";
  if (spanName.startsWith("workflow.task.execute_sql.")) return "bg-sky-600";
  if (spanName.startsWith("workflow.task.omni_query.")) return "bg-blue-600";
  if (spanName.startsWith("workflow.task.loop.")) return "bg-violet-600";
  if (spanName.startsWith("workflow.task.formatter.")) return "bg-fuchsia-600";
  if (spanName.startsWith("workflow.task.sub_workflow.")) return "bg-lime-600";
  if (spanName.startsWith("workflow.task.")) return "bg-green-500";

  // General workflow operations - default green
  if (spanName.startsWith("workflow.")) return "bg-green-500";

  // Agent operations - various blues
  if (spanName.includes("routing_agent")) return "bg-cyan-500";
  if (spanName.includes("fallback_agent")) return "bg-slate-500";
  if (spanName.includes("default_agent")) return "bg-blue-500";
  if (spanName === "agent.run_agent") return "bg-indigo-600";
  if (spanName === "agent.get_global_context") return "bg-green-500";
  if (spanName === "agent.load_config") return "bg-blue-400";
  if (spanName.startsWith("agent.")) return "bg-blue-500";

  // Data operations - teal
  if (spanName === "load" || spanName === "count_rows") return "bg-teal-500";

  return "bg-primary";
}
