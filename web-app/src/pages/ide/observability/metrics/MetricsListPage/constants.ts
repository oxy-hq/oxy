import {
  Zap,
  Code,
  HelpCircle,
  MessageCircle,
  Database,
  LucideBot,
  LucideWorkflow,
} from "lucide-react";
import { createElement } from "react";

export const DAYS_OPTIONS = [
  { value: 7, label: "7d" },
  { value: 30, label: "30d" },
  { value: 90, label: "90d" },
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
    color: "text-blue-400",
    bgColor: "bg-blue-500/10",
    icon: createElement(LucideBot, { className: "h-4 w-4" }),
  },
  workflow: {
    label: "Workflow",
    color: "text-purple-400",
    bgColor: "bg-purple-500/10",
    icon: createElement(LucideWorkflow, { className: "h-4 w-4" }),
  },
  task: {
    label: "Task",
    color: "text-green-400",
    bgColor: "bg-green-500/10",
    icon: createElement(Zap, { className: "h-4 w-4" }),
  },
};

export const CONTEXT_TYPE_CONFIG: Record<
  string,
  { label: string; color: string; bgColor: string; icon: React.ReactNode }
> = {
  sql: {
    label: "SQL",
    color: "text-cyan-400",
    bgColor: "bg-cyan-500/10",
    icon: createElement(Code, { className: "h-4 w-4" }),
  },
  question: {
    label: "Question",
    color: "text-amber-400",
    bgColor: "bg-amber-500/10",
    icon: createElement(HelpCircle, { className: "h-4 w-4" }),
  },
  response: {
    label: "Response",
    color: "text-emerald-400",
    bgColor: "bg-emerald-500/10",
    icon: createElement(MessageCircle, { className: "h-4 w-4" }),
  },
  semantic: {
    label: "Semantic",
    color: "text-orange-400",
    bgColor: "bg-orange-500/10",
    icon: createElement(Database, { className: "h-4 w-4" }),
  },
};

export function getRankColor(rank: number): string {
  if (rank === 1) return "bg-yellow-500";
  if (rank <= 3) return "bg-primary";
  return "bg-primary/70";
}
