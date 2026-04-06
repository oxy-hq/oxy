import { Code, Database, HelpCircle, LucideBot, LucideWorkflow, MessageCircle } from "lucide-react";
import { createElement } from "react";

export const DAYS_OPTIONS = [
  { value: 7, label: "7d" },
  { value: 30, label: "30d" },
  { value: 90, label: "90d" }
] as const;

export type DaysValue = (typeof DAYS_OPTIONS)[number]["value"];

export const SOURCE_TYPE_CONFIG: Record<
  string,
  { label: string; color: string; bgColor: string; icon: React.ReactNode }
> = {
  Agent: {
    label: "Agent",
    color: "text-info",
    bgColor: "bg-info/10 border-info/20",
    icon: createElement(LucideBot, { className: "h-3.5 w-3.5" })
  },
  Workflow: {
    label: "Workflow",
    color: "text-vis-purple",
    bgColor: "bg-vis-purple/10 border-vis-purple/20",
    icon: createElement(LucideWorkflow, { className: "h-3.5 w-3.5" })
  }
};

export const CONTEXT_TYPE_CONFIG: Record<
  string,
  { label: string; color: string; bgColor: string; icon: React.ReactNode }
> = {
  SQL: {
    label: "SQL",
    color: "text-vis-cyan",
    bgColor: "bg-vis-cyan/10",
    icon: createElement(Code, { className: "h-3 w-3" })
  },
  sql: {
    label: "SQL",
    color: "text-vis-cyan",
    bgColor: "bg-vis-cyan/10",
    icon: createElement(Code, { className: "h-3 w-3" })
  },
  Question: {
    label: "Question",
    color: "text-warning",
    bgColor: "bg-warning/10",
    icon: createElement(HelpCircle, { className: "h-3 w-3" })
  },
  question: {
    label: "Question",
    color: "text-warning",
    bgColor: "bg-warning/10",
    icon: createElement(HelpCircle, { className: "h-3 w-3" })
  },
  Response: {
    label: "Response",
    color: "text-success",
    bgColor: "bg-success/10",
    icon: createElement(MessageCircle, { className: "h-3 w-3" })
  },
  response: {
    label: "Response",
    color: "text-success",
    bgColor: "bg-success/10",
    icon: createElement(MessageCircle, { className: "h-3 w-3" })
  },
  SemanticQuery: {
    label: "Semantic",
    color: "text-vis-orange",
    bgColor: "bg-vis-orange/10",
    icon: createElement(Database, { className: "h-3 w-3" })
  },
  semantic: {
    label: "Semantic",
    color: "text-vis-orange",
    bgColor: "bg-vis-orange/10",
    icon: createElement(Database, { className: "h-3 w-3" })
  }
};
