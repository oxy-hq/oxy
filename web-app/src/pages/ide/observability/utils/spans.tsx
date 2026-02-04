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
    "workflow.task.semantic_query.get_sql_from_cubejs": "Generate SQL from CubeJS",
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
    "sql.execute": "Execute SQL",
    "validate_sql.execute": "Validate SQL",
    "workflow.execute": "Execute Workflow",
    "retrieval.execute": "Retrieve Data",
    "omni_query.execute": "Execute Omni Query",
    "visualize.execute": "Visualize Data",
    "create_data_app.execute": "Create Data App",
    "tool_launcher.execute": "Launch Tool",
    "semantic_query.execute": "Execute Semantic Query",

    // Data operations
    load: "Load Data",
    count_rows: "Count Rows"
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
