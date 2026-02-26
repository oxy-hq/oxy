import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import type { Block } from "@/services/types";
import { type TaskConfig, TaskType } from "@/stores/useWorkflow";

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 60);
}

function getTaskName(step: Step, index: number): string {
  if (step.objective) {
    const slug = slugify(step.objective);
    if (slug) return slug;
  }
  return `step_${index + 1}`;
}

function findBlockOfType(blocks: Block[], type: string): Block | undefined {
  return blocks.find((b) => b.type === type);
}

const FILTER_OPS = [
  "eq",
  "neq",
  "gt",
  "gte",
  "lt",
  "lte",
  "in",
  "not_in",
  "in_date_range",
  "not_in_date_range"
] as const;

interface SemanticQueryJson {
  topic?: string;
  measures?: string[];
  dimensions?: string[];
  filters?: Array<Record<string, unknown>>;
  orders?: Array<{ field: string; direction: string }>;
  limit?: number;
  offset?: number;
}

function parseSemanticQueryJson(jsonStr: string): SemanticQueryJson | null {
  try {
    return JSON.parse(jsonStr) as SemanticQueryJson;
  } catch {
    return null;
  }
}

function convertSemanticFilters(
  rawFilters: Array<Record<string, unknown>>
): Array<{ field: string; op: string; value: string | number | boolean | string[] }> {
  return rawFilters.flatMap((f) => {
    const field = f.field as string;
    if (!field) return [];
    for (const op of FILTER_OPS) {
      if (op in f) {
        const filterVal = f[op] as Record<string, unknown> | undefined;
        if (filterVal && "value" in filterVal) {
          return [{ field, op, value: filterVal.value as string | number | boolean | string[] }];
        }
        if (filterVal && "values" in filterVal) {
          return [{ field, op, value: filterVal.values as string[] }];
        }
      }
    }
    return [];
  });
}

function buildSemanticQueryTask(
  name: string,
  sqBlock: Block & { type: "semantic_query" }
): TaskConfig {
  const parsed = parseSemanticQueryJson(sqBlock.semantic_query);
  if (!parsed || !parsed.topic) {
    return {
      name,
      type: TaskType.SEMANTIC_QUERY,
      database: "default",
      topic: ""
    };
  }
  const task: TaskConfig = {
    name,
    type: TaskType.SEMANTIC_QUERY,
    database: "default",
    topic: parsed.topic,
    ...(parsed.measures?.length && { measures: parsed.measures }),
    ...(parsed.dimensions?.length && { dimensions: parsed.dimensions }),
    ...(parsed.filters?.length && { filters: convertSemanticFilters(parsed.filters) }),
    ...(parsed.orders?.length && { orders: parsed.orders }),
    ...(parsed.limit != null && { limit: parsed.limit }),
    ...(parsed.offset != null && { offset: parsed.offset })
  };
  return task;
}

function convertStep(step: Step, index: number): TaskConfig | null {
  const name = getTaskName(step, index);

  switch (step.step_type) {
    case "semantic_query": {
      const sqBlock = findBlockOfType(step.childrenBlocks, "semantic_query");
      if (sqBlock && sqBlock.type === "semantic_query") {
        return buildSemanticQueryTask(name, sqBlock);
      }
      return {
        name,
        type: TaskType.SEMANTIC_QUERY,
        database: "default",
        topic: step.objective || ""
      };
    }
    case "query": {
      const sqlBlock = findBlockOfType(step.childrenBlocks, "sql");
      if (sqlBlock && sqlBlock.type === "sql") {
        return {
          name,
          type: TaskType.EXECUTE_SQL,
          database: sqlBlock.database,
          sql_query: sqlBlock.sql_query
        };
      }
      const sqBlock = findBlockOfType(step.childrenBlocks, "semantic_query");
      if (sqBlock && sqBlock.type === "semantic_query") {
        return buildSemanticQueryTask(name, sqBlock);
      }
      return null;
    }
    case "visualize": {
      // Visualize task type is not yet supported in backend workflow execution.
      return null;
    }
    case "insight": {
      // Insight steps cannot be replayed as agent tasks without knowing the agent ref.
      return null;
    }
    case "end": {
      // End/summary steps become formatter tasks with the generated text
      const textBlock = findBlockOfType(step.childrenBlocks, "text");
      if (textBlock && textBlock.type === "text") {
        return {
          name,
          type: TaskType.FORMATTER,
          template: textBlock.content
        };
      }
      return null;
    }
    case "subflow":
      // Subflow steps cannot be replayed as workflow tasks without knowing the src path.
      return null;
    default:
      return null;
  }
}

export function buildStepDagMapping(steps: Step[]): Map<string, string> {
  const mapping = new Map<string, string>();
  for (let i = 0; i < steps.length; i++) {
    const step = steps[i];
    if (
      step.error ||
      step.step_type === "plan" ||
      step.step_type === "idle" ||
      step.step_type === "save_automation" ||
      step.step_type === "build_app"
    ) {
      continue;
    }
    const task = convertStep(step, i);
    if (task) {
      mapping.set(step.id, task.name);
    }
  }
  return mapping;
}

export function convertReasoningToTasks(steps: Step[]): TaskConfig[] {
  const tasks: TaskConfig[] = [];
  for (let i = 0; i < steps.length; i++) {
    const step = steps[i];
    // Skip plan, idle, save_automation, build_app, and errored steps
    if (
      step.error ||
      step.step_type === "plan" ||
      step.step_type === "idle" ||
      step.step_type === "save_automation" ||
      step.step_type === "build_app"
    ) {
      continue;
    }
    const task = convertStep(step, i);
    if (task) {
      tasks.push(task);
    }
  }
  return tasks;
}

export function generateAutomationName(steps: Step[], userQuestion?: string): string {
  if (userQuestion) {
    const slug = slugify(userQuestion);
    if (slug) return slug;
  }
  const firstMeaningful = steps.find(
    (s) => s.step_type !== "plan" && s.step_type !== "idle" && s.objective
  );
  if (firstMeaningful?.objective) {
    const slug = slugify(firstMeaningful.objective);
    if (slug) return slug;
  }
  return `automation_${Date.now()}`;
}

export function generateAutomationDescription(steps: Step[]): string {
  const objectives = steps
    .filter((s) => s.objective && s.step_type !== "plan" && s.step_type !== "idle")
    .map((s) => s.objective)
    .filter(Boolean);
  if (objectives.length > 0) {
    return objectives.join("; ");
  }
  return "Auto-generated automation from reasoning steps";
}
