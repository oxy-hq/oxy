import {
  BarChart3,
  Blocks,
  Bot,
  CodeXml,
  Compass,
  Flag,
  GitBranch,
  Globe,
  Lightbulb,
  Save
} from "lucide-react";
import type { ElementType } from "react";

export const STEP_ICON: Record<string, ElementType> = {
  plan: Compass,
  route: GitBranch,
  semantic_query: Globe,
  looker_query: Globe,
  query: CodeXml,
  insight: Lightbulb,
  visualize: BarChart3,
  end: Flag,
  subflow: GitBranch,
  save_automation: Save,
  build_app: Blocks,
  idle: Bot
};

export const STEP_COLOR_DOT: Record<string, string> = {
  plan: "bg-node-plan",
  route: "bg-node-plan",
  semantic_query: "bg-node-query",
  looker_query: "bg-node-query",
  query: "bg-node-query",
  insight: "bg-node-agent",
  visualize: "bg-node-query",
  end: "bg-node-formatter",
  subflow: "bg-node-plan",
  save_automation: "bg-node-plan",
  build_app: "bg-node-plan",
  idle: "bg-muted-foreground"
};

export const STEP_COLOR_TEXT: Record<string, string> = {
  plan: "text-node-plan",
  route: "text-node-plan",
  semantic_query: "text-node-query",
  looker_query: "text-node-query",
  query: "text-node-query",
  insight: "text-node-agent",
  visualize: "text-node-query",
  end: "text-node-formatter",
  subflow: "text-node-plan",
  save_automation: "text-node-plan",
  build_app: "text-node-plan",
  idle: "text-muted-foreground"
};

export const STEP_COLOR_BG: Record<string, string> = {
  plan: "bg-node-plan/12",
  route: "bg-node-plan/12",
  semantic_query: "bg-node-query/12",
  looker_query: "bg-node-query/12",
  query: "bg-node-query/12",
  insight: "bg-node-agent/12",
  visualize: "bg-node-query/12",
  end: "bg-node-formatter/12",
  subflow: "bg-node-plan/12",
  save_automation: "bg-node-plan/12",
  build_app: "bg-node-plan/12",
  idle: "bg-muted/12"
};

export const STEP_COLOR_BORDER: Record<string, string> = {
  plan: "border-node-plan/50",
  route: "border-node-plan/50",
  semantic_query: "border-node-query/50",
  looker_query: "border-node-query/50",
  query: "border-node-query/50",
  insight: "border-node-agent/50",
  visualize: "border-node-query/50",
  end: "border-node-formatter/50",
  subflow: "border-node-plan/50",
  save_automation: "border-node-plan/50",
  build_app: "border-node-plan/50",
  idle: "border-muted-foreground/50"
};
