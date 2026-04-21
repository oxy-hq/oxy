import {
  BarChart3,
  Code,
  Database,
  HelpCircle,
  LucideBot,
  LucideWorkflow,
  MessageCircle,
  Zap
} from "lucide-react";
import { createElement } from "react";

export const DAYS_OPTIONS = [
  { value: 7, label: "7d" },
  { value: 30, label: "30d" },
  { value: 90, label: "90d" }
] as const;

export type DaysValue = (typeof DAYS_OPTIONS)[number]["value"];
export type ViewMode = "grid" | "list";

export const METRICS_PAGE_SIZE = 12;

export const SOURCE_TYPE_CONFIG: Record<
  string,
  { label: string; color: string; bgColor: string; icon: React.ReactNode }
> = {
  agent: {
    label: "Agent",
    color: "text-info",
    bgColor: "bg-info/10",
    icon: createElement(LucideBot, { className: "h-4 w-4" })
  },
  workflow: {
    label: "Workflow",
    color: "text-vis-purple",
    bgColor: "bg-vis-purple/10",
    icon: createElement(LucideWorkflow, { className: "h-4 w-4" })
  },
  task: {
    label: "Task",
    color: "text-success",
    bgColor: "bg-success/10",
    icon: createElement(Zap, { className: "h-4 w-4" })
  },
  analytics: {
    label: "Analytics",
    color: "text-vis-violet",
    bgColor: "bg-vis-violet/10",
    icon: createElement(BarChart3, { className: "h-4 w-4" })
  }
};

export const CONTEXT_TYPE_CONFIG: Record<
  string,
  { label: string; color: string; bgColor: string; icon: React.ReactNode }
> = {
  sql: {
    label: "SQL",
    color: "text-vis-cyan",
    bgColor: "bg-vis-cyan/10",
    icon: createElement(Code, { className: "h-4 w-4" })
  },
  question: {
    label: "Question",
    color: "text-warning",
    bgColor: "bg-warning/10",
    icon: createElement(HelpCircle, { className: "h-4 w-4" })
  },
  response: {
    label: "Response",
    color: "text-success",
    bgColor: "bg-success/10",
    icon: createElement(MessageCircle, { className: "h-4 w-4" })
  },
  semantic: {
    label: "Semantic",
    color: "text-vis-orange",
    bgColor: "bg-vis-orange/10",
    icon: createElement(Database, { className: "h-4 w-4" })
  }
};

export function getRankColor(rank: number): string {
  if (rank === 1) return "bg-warning";
  if (rank <= 3) return "bg-primary";
  return "bg-primary/70";
}
