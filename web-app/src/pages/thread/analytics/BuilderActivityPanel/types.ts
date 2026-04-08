import { parse as parseYaml } from "yaml";

// ── YAML types ───────────────────────────────────────────────────────────────

export type ViewField = {
  name: string;
  type?: string;
  description?: string;
  expr?: string;
};

export type SemanticView = {
  name?: string;
  description?: string;
  datasource?: string;
  table?: string;
  dimensions?: ViewField[];
  measures?: ViewField[];
};

export type AppTask = {
  name: string;
  type?: string;
  database?: string;
  sql_query?: string;
  sql_file?: string;
};

export type AppDisplayItem = {
  type: string;
  // control
  name?: string;
  control_type?: string;
  label?: string;
  source?: string;
  // chart / table
  title?: string;
  data?: string;
  x?: string;
  y?: string;
  // row
  columns?: number;
  children?: AppDisplayItem[];
  // markdown
  content?: string;
};

export type DataApp = {
  name?: string;
  description?: string;
  tasks?: AppTask[];
  display?: AppDisplayItem[];
};

export type WorkflowTask = {
  name: string;
  type: string;
  database?: string;
  sql_query?: string;
  sql_file?: string;
  template?: string;
};

export type WorkflowConfig = {
  name?: string;
  description?: string;
  tasks?: WorkflowTask[];
};

export type AgentTool = {
  type: string;
  name?: string;
  database?: string;
};

export type AgentContext = {
  name: string;
  type: string;
};

export type AgentConfig = {
  name?: string;
  model?: string;
  description?: string;
  agent_type?: string;
  tools?: AgentTool[];
  context?: AgentContext[];
};

export type TopicConfig = {
  name?: string;
  description?: string;
  views?: string[];
};

export type AwTransition = {
  type: string;
  database?: string;
};

export type AwConfig = {
  model?: string;
  start?: { mode?: string; instruction?: string; next?: string[] };
  end?: { output_artifact?: string; mode?: string };
  transitions?: AwTransition[];
};

export type TestCase = {
  name?: string;
  prompt: string;
  expected: string;
  tags?: string[];
  tool?: string;
};

export type TestSettings = {
  concurrency?: number;
  runs?: number;
  judge_model?: string;
};

export type TestFileConfig = {
  name?: string;
  target?: string;
  settings?: TestSettings;
  cases?: TestCase[];
};

// ── Diff types ───────────────────────────────────────────────────────────────

export type FieldDiffStatus = "added" | "removed" | "modified" | "unchanged";

export type FieldDiff = {
  name: string;
  status: FieldDiffStatus;
  kind: "dim" | "measure";
  field: ViewField;
  oldField?: ViewField;
  changes?: string[];
};

export type AppItemKind =
  | "task"
  | "display"
  | "tool"
  | "context"
  | "view"
  | "transition"
  | "test_case";

export type AppItemDiff = {
  key: string;
  status: FieldDiffStatus;
  kind: AppItemKind;
  label: string;
  title: string;
  subtitle?: string;
  changes?: string[];
  children?: AppItemDiff[];
};

export type ItemGroup = { label: string; items: AppItemDiff[] };

// ── Parse helpers ─────────────────────────────────────────────────────────────

export function tryParseView(content: string): SemanticView | null {
  try {
    const parsed = parseYaml(content);
    if (parsed && typeof parsed === "object" && (parsed.dimensions || parsed.measures)) {
      return parsed as SemanticView;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

export function tryParseApp(content: string): DataApp | null {
  try {
    const parsed = parseYaml(content);
    if (parsed && typeof parsed === "object" && (parsed.tasks || parsed.display)) {
      return parsed as DataApp;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

export function tryParseWorkflow(content: string): WorkflowConfig | null {
  try {
    const parsed = parseYaml(content);
    if (parsed && typeof parsed === "object" && Array.isArray(parsed.tasks)) {
      return parsed as WorkflowConfig;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

export function tryParseAgent(content: string): AgentConfig | null {
  try {
    const parsed = parseYaml(content);
    if (
      parsed &&
      typeof parsed === "object" &&
      (parsed.tools !== undefined || parsed.agent_type !== undefined || parsed.model !== undefined)
    ) {
      return parsed as AgentConfig;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

export function tryParseTopic(content: string): TopicConfig | null {
  try {
    const parsed = parseYaml(content);
    if (parsed && typeof parsed === "object" && Array.isArray(parsed.views)) {
      return parsed as TopicConfig;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

export function tryParseAw(content: string): AwConfig | null {
  try {
    const parsed = parseYaml(content);
    if (parsed && typeof parsed === "object" && (parsed.transitions || parsed.start)) {
      return parsed as AwConfig;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

// ── Diff helpers ──────────────────────────────────────────────────────────────

/** Find dimension names referenced in a field's expr. */
export function findRelatedDims(field: ViewField, allDims: ViewField[]): string[] {
  if (!field.expr) return [];
  const expr = field.expr.toLowerCase();
  return allDims.filter((d) => d.name && expr.includes(d.name.toLowerCase())).map((d) => d.name);
}

export function diffFields(
  oldFields: ViewField[],
  newFields: ViewField[],
  kind: "dim" | "measure"
): FieldDiff[] {
  const oldMap = new Map(oldFields.map((f) => [f.name, f]));
  const newMap = new Map(newFields.map((f) => [f.name, f]));
  const result: FieldDiff[] = [];

  for (const f of newFields) {
    const old = oldMap.get(f.name);
    if (!old) {
      result.push({ name: f.name, status: "added", kind, field: f });
    } else {
      const changes: string[] = [];
      if (old.type !== f.type) changes.push(`type: ${old.type ?? "–"} → ${f.type ?? "–"}`);
      if (old.description !== f.description) changes.push("description updated");
      if (old.expr !== f.expr) changes.push(`expr: ${old.expr ?? "–"} → ${f.expr ?? "–"}`);
      result.push({
        name: f.name,
        status: changes.length > 0 ? "modified" : "unchanged",
        kind,
        field: f,
        oldField: old,
        changes: changes.length > 0 ? changes : undefined
      });
    }
  }

  for (const f of oldFields) {
    if (!newMap.has(f.name)) {
      result.push({ name: f.name, status: "removed", kind, field: f });
    }
  }

  return result;
}

function displayLabel(type: string): string {
  const labels: Record<string, string> = {
    line_chart: "Line Chart",
    bar_chart: "Bar Chart",
    pie_chart: "Pie Chart",
    table: "Table",
    markdown: "Markdown",
    row: "Row",
    control: "Control"
  };
  return labels[type] ?? type;
}

function displayKey(item: AppDisplayItem, idx: number): string {
  if (item.name) return `ctrl:${item.name}`;
  if (item.title) return `${item.type}:${item.title}`;
  if (item.content) return `md:${item.content.slice(0, 30)}`;
  return `${item.type}:${idx}`;
}

function displayTitle(item: AppDisplayItem): string {
  return (
    item.title ?? item.name ?? item.content?.split("\n")[0]?.replace(/^#+\s*/, "") ?? item.type
  );
}

function displaySubtitle(item: AppDisplayItem): string | undefined {
  if (item.data) return `data: ${item.data}`;
  if (item.source) return `source: ${item.source}`;
  if (item.type === "row" && item.columns) return `${item.columns} columns`;
  return undefined;
}

function diffDisplayItems(oldItems: AppDisplayItem[], newItems: AppDisplayItem[]): AppItemDiff[] {
  const result: AppItemDiff[] = [];
  const oldMap = new Map(oldItems.map((d, i) => [displayKey(d, i), d]));
  const newKeys = new Set(newItems.map((d, i) => displayKey(d, i)));

  newItems.forEach((d, i) => {
    const key = displayKey(d, i);
    const old = oldMap.get(key);
    if (d.type === "row") {
      const childDiffs = diffDisplayItems(old?.children ?? [], d.children ?? []);
      const changedChildren = childDiffs.filter((c) => c.status !== "unchanged");
      const allChildrenAdded = childDiffs.every((c) => c.status === "added");
      const status: FieldDiffStatus = !old
        ? "added"
        : changedChildren.length > 0
          ? "modified"
          : "unchanged";
      const displayChildren = !old
        ? allChildrenAdded
          ? childDiffs
          : childDiffs.filter((c) => c.status === "added")
        : changedChildren;
      result.push({
        key: `display:${key}`,
        status,
        kind: "display",
        label: "Row",
        title: d.columns ? `${d.columns} col` : "Row",
        children: displayChildren.length > 0 ? displayChildren : undefined
      });
    } else if (!old) {
      result.push({
        key: `display:${key}`,
        status: "added",
        kind: "display",
        label: displayLabel(d.type),
        title: displayTitle(d),
        subtitle: displaySubtitle(d)
      });
    } else {
      const changed = JSON.stringify(old) !== JSON.stringify(d);
      result.push({
        key: `display:${key}`,
        status: changed ? "modified" : "unchanged",
        kind: "display",
        label: displayLabel(d.type),
        title: displayTitle(d),
        subtitle: displaySubtitle(d),
        changes: changed ? ["content updated"] : undefined
      });
    }
  });

  oldItems.forEach((d, i) => {
    if (!newKeys.has(displayKey(d, i))) {
      result.push({
        key: `display:${displayKey(d, i)}`,
        status: "removed",
        kind: "display",
        label: d.type === "row" ? "Row" : displayLabel(d.type),
        title: d.type === "row" ? (d.columns ? `${d.columns} col` : "Row") : displayTitle(d)
      });
    }
  });

  return result;
}

export function diffAppItems(oldApp: DataApp | null, newApp: DataApp): AppItemDiff[] {
  const result: AppItemDiff[] = [];

  const oldTasks = oldApp?.tasks ?? [];
  const newTasks = newApp.tasks ?? [];
  const oldTaskMap = new Map(oldTasks.map((t) => [t.name, t]));
  const newTaskMap = new Map(newTasks.map((t) => [t.name, t]));

  for (const t of newTasks) {
    const old = oldTaskMap.get(t.name);
    if (!old) {
      result.push({
        key: `task:${t.name}`,
        status: "added",
        kind: "task",
        label: "Task",
        title: t.name,
        subtitle: t.database ? `db: ${t.database}` : undefined
      });
    } else {
      const changes: string[] = [];
      if (old.database !== t.database)
        changes.push(`db: ${old.database ?? "–"} → ${t.database ?? "–"}`);
      if ((old.sql_query ?? old.sql_file) !== (t.sql_query ?? t.sql_file))
        changes.push("query updated");
      result.push({
        key: `task:${t.name}`,
        status: changes.length > 0 ? "modified" : "unchanged",
        kind: "task",
        label: "Task",
        title: t.name,
        subtitle: t.database ? `db: ${t.database}` : undefined,
        changes: changes.length > 0 ? changes : undefined
      });
    }
  }
  for (const t of oldTasks) {
    if (!newTaskMap.has(t.name)) {
      result.push({
        key: `task:${t.name}`,
        status: "removed",
        kind: "task",
        label: "Task",
        title: t.name
      });
    }
  }

  result.push(...diffDisplayItems(oldApp?.display ?? [], newApp.display ?? []));

  return result;
}

export function diffAppTasks(oldApp: DataApp | null, newApp: DataApp): AppItemDiff[] {
  return diffAppItems(oldApp, newApp).filter((d) => d.kind === "task");
}

export function diffAppDisplays(oldApp: DataApp | null, newApp: DataApp): AppItemDiff[] {
  return diffAppItems(oldApp, newApp).filter((d) => d.kind === "display");
}

export function workflowKind(filePath: string): string {
  if (filePath.endsWith(".procedure.yml") || filePath.endsWith(".procedure.yaml"))
    return "Procedure";
  if (filePath.endsWith(".automation.yml") || filePath.endsWith(".automation.yaml"))
    return "Automation";
  return "Workflow";
}

export function diffWorkflowTasks(
  oldWf: WorkflowConfig | null,
  newWf: WorkflowConfig
): AppItemDiff[] {
  const result: AppItemDiff[] = [];
  const oldTasks = oldWf?.tasks ?? [];
  const newTasks = newWf.tasks ?? [];
  const oldTaskMap = new Map(oldTasks.map((t) => [t.name, t]));
  const newTaskMap = new Map(newTasks.map((t) => [t.name, t]));

  for (const t of newTasks) {
    const old = oldTaskMap.get(t.name);
    const label =
      t.type === "execute_sql" ? "SQL Task" : t.type === "formatter" ? "Formatter" : "Task";
    if (!old) {
      result.push({
        key: `task:${t.name}`,
        status: "added",
        kind: "task",
        label,
        title: t.name,
        subtitle: t.database ? `db: ${t.database}` : undefined
      });
    } else {
      const changes: string[] = [];
      if (old.type !== t.type) changes.push(`type: ${old.type} → ${t.type}`);
      if (old.database !== t.database)
        changes.push(`db: ${old.database ?? "–"} → ${t.database ?? "–"}`);
      if ((old.sql_query ?? old.sql_file) !== (t.sql_query ?? t.sql_file))
        changes.push("query updated");
      if (old.template !== t.template) changes.push("template updated");
      if (changes.length === 0 && JSON.stringify(old) !== JSON.stringify(t))
        changes.push("updated");
      result.push({
        key: `task:${t.name}`,
        status: changes.length > 0 ? "modified" : "unchanged",
        kind: "task",
        label,
        title: t.name,
        subtitle: t.database ? `db: ${t.database}` : undefined,
        changes: changes.length > 0 ? changes : undefined
      });
    }
  }
  for (const t of oldTasks) {
    if (!newTaskMap.has(t.name))
      result.push({
        key: `task:${t.name}`,
        status: "removed",
        kind: "task",
        label: "Task",
        title: t.name
      });
  }
  return result;
}

export function diffAgentItems(oldAgent: AgentConfig | null, newAgent: AgentConfig): AppItemDiff[] {
  const result: AppItemDiff[] = [];
  const toolKey = (t: AgentTool) => t.name ?? t.type;

  const oldTools = oldAgent?.tools ?? [];
  const newTools = newAgent.tools ?? [];
  const oldToolMap = new Map(oldTools.map((t) => [toolKey(t), t]));
  const newToolMap = new Map(newTools.map((t) => [toolKey(t), t]));

  for (const t of newTools) {
    const key = toolKey(t);
    const old = oldToolMap.get(key);
    if (!old) {
      result.push({
        key: `tool:${key}`,
        status: "added",
        kind: "tool",
        label: "Tool",
        title: key,
        subtitle: t.database ? `db: ${t.database}` : undefined
      });
    } else {
      const changes: string[] = [];
      if (old.database !== t.database)
        changes.push(`db: ${old.database ?? "–"} → ${t.database ?? "–"}`);
      result.push({
        key: `tool:${key}`,
        status: changes.length > 0 ? "modified" : "unchanged",
        kind: "tool",
        label: "Tool",
        title: key,
        subtitle: t.database ? `db: ${t.database}` : undefined,
        changes: changes.length > 0 ? changes : undefined
      });
    }
  }
  for (const t of oldTools) {
    const key = toolKey(t);
    if (!newToolMap.has(key))
      result.push({
        key: `tool:${key}`,
        status: "removed",
        kind: "tool",
        label: "Tool",
        title: key
      });
  }

  const oldCtx = oldAgent?.context ?? [];
  const newCtx = newAgent.context ?? [];
  const oldCtxMap = new Map(oldCtx.map((c) => [c.name, c]));
  const newCtxMap = new Map(newCtx.map((c) => [c.name, c]));

  for (const c of newCtx) {
    const old = oldCtxMap.get(c.name);
    if (!old) {
      result.push({
        key: `ctx:${c.name}`,
        status: "added",
        kind: "context",
        label: "Context",
        title: c.name,
        subtitle: c.type
      });
    } else {
      const changed = JSON.stringify(old) !== JSON.stringify(c);
      result.push({
        key: `ctx:${c.name}`,
        status: changed ? "modified" : "unchanged",
        kind: "context",
        label: "Context",
        title: c.name,
        subtitle: c.type,
        changes: changed ? ["updated"] : undefined
      });
    }
  }
  for (const c of oldCtx) {
    if (!newCtxMap.has(c.name))
      result.push({
        key: `ctx:${c.name}`,
        status: "removed",
        kind: "context",
        label: "Context",
        title: c.name
      });
  }

  return result;
}

export function diffTopicViews(oldTopic: TopicConfig | null, newTopic: TopicConfig): AppItemDiff[] {
  const oldViews = new Set(oldTopic?.views ?? []);
  const newViews = newTopic.views ?? [];
  const result: AppItemDiff[] = newViews.map((v) => ({
    key: `view:${v}`,
    status: oldViews.has(v) ? ("unchanged" as const) : ("added" as const),
    kind: "view" as const,
    label: "View",
    title: v
  }));
  for (const v of Array.from(oldViews)) {
    if (!newViews.includes(v))
      result.push({ key: `view:${v}`, status: "removed", kind: "view", label: "View", title: v });
  }
  return result;
}

export function tryParseTest(content: string): TestFileConfig | null {
  try {
    const parsed = parseYaml(content);
    if (parsed && typeof parsed === "object" && Array.isArray(parsed.cases)) {
      return parsed as TestFileConfig;
    }
  } catch {
    // not valid YAML
  }
  return null;
}

export function diffTestCases(
  oldTest: TestFileConfig | null,
  newTest: TestFileConfig
): AppItemDiff[] {
  const result: AppItemDiff[] = [];
  const oldCases = oldTest?.cases ?? [];
  const newCases = newTest.cases ?? [];
  const caseKey = (c: TestCase, i: number) => c.name ?? `case:${i}`;
  const oldMap = new Map(oldCases.map((c, i) => [caseKey(c, i), c]));
  const newKeys = new Set(newCases.map((c, i) => caseKey(c, i)));

  newCases.forEach((c, i) => {
    const key = caseKey(c, i);
    const old = oldMap.get(key);
    const title = c.name ?? c.prompt.slice(0, 40);
    const subtitleParts = [
      c.tags && c.tags.length > 0 ? c.tags.join(", ") : undefined,
      c.tool ? `tool: ${c.tool}` : undefined
    ].filter(Boolean);
    const subtitle = subtitleParts.length > 0 ? subtitleParts.join(" · ") : undefined;
    if (!old) {
      result.push({
        key: `case:${key}`,
        status: "added",
        kind: "test_case",
        label: "Case",
        title,
        subtitle
      });
    } else {
      const changes: string[] = [];
      if (old.prompt !== c.prompt) changes.push("prompt updated");
      if (old.expected !== c.expected) changes.push("expected updated");
      if (old.tool !== c.tool) changes.push(`tool: ${old.tool ?? "–"} → ${c.tool ?? "–"}`);
      result.push({
        key: `case:${key}`,
        status: changes.length > 0 ? "modified" : "unchanged",
        kind: "test_case",
        label: "Case",
        title,
        subtitle,
        changes: changes.length > 0 ? changes : undefined
      });
    }
  });

  oldCases.forEach((c, i) => {
    const key = caseKey(c, i);
    if (!newKeys.has(key))
      result.push({
        key: `case:${key}`,
        status: "removed",
        kind: "test_case",
        label: "Case",
        title: c.name ?? c.prompt.slice(0, 40)
      });
  });

  return result;
}

export function diffAwTransitions(oldAw: AwConfig | null, newAw: AwConfig): AppItemDiff[] {
  const transKey = (t: AwTransition) => (t.database ? `${t.type}:${t.database}` : t.type);
  const oldTrans = oldAw?.transitions ?? [];
  const newTrans = newAw.transitions ?? [];
  const oldTransMap = new Map(oldTrans.map((t) => [transKey(t), t]));
  const newTransKeys = new Set(newTrans.map(transKey));

  const result: AppItemDiff[] = newTrans.map((t) => ({
    key: `trans:${transKey(t)}`,
    status: oldTransMap.has(transKey(t)) ? ("unchanged" as const) : ("added" as const),
    kind: "transition" as const,
    label: "Transition",
    title: t.type,
    subtitle: t.database ? `db: ${t.database}` : undefined
  }));
  for (const t of oldTrans) {
    if (!newTransKeys.has(transKey(t)))
      result.push({
        key: `trans:${transKey(t)}`,
        status: "removed",
        kind: "transition",
        label: "Transition",
        title: t.type
      });
  }
  return result;
}
