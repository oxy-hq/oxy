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
  plan: "bg-node-plan/8",
  route: "bg-node-plan/8",
  semantic_query: "bg-node-query/8",
  query: "bg-node-query/8",
  insight: "bg-node-agent/8",
  visualize: "bg-node-query/8",
  end: "bg-node-formatter/8",
  subflow: "bg-node-plan/8",
  save_automation: "bg-node-plan/8",
  build_app: "bg-node-plan/8",
  idle: "bg-muted/8"
};

export const STEP_COLOR_BORDER: Record<string, string> = {
  plan: "border-node-plan/30",
  route: "border-node-plan/30",
  semantic_query: "border-node-query/30",
  query: "border-node-query/30",
  insight: "border-node-agent/30",
  visualize: "border-node-query/30",
  end: "border-node-formatter/30",
  subflow: "border-node-plan/30",
  save_automation: "border-node-plan/30",
  build_app: "border-node-plan/30",
  idle: "border-muted-foreground/30"
};
