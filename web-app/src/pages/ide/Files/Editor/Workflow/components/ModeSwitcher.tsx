import { Code, FileText, Play } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { WorkflowViewMode } from "./types";

interface ModeSwitcherProps {
  viewMode: WorkflowViewMode;
  onViewModeChange: (mode: WorkflowViewMode) => void;
}

const ModeSwitcher = ({ viewMode, onViewModeChange }: ModeSwitcherProps) => (
  <Tabs
    value={viewMode}
    onValueChange={(value: string) => {
      if (Object.values(WorkflowViewMode).includes(value as WorkflowViewMode)) {
        onViewModeChange(value as WorkflowViewMode);
      }
    }}
  >
    <TabsList>
      <TabsTrigger value={WorkflowViewMode.Output} aria-label='Output view'>
        <Play />
      </TabsTrigger>
      <TabsTrigger value={WorkflowViewMode.Editor} aria-label='Editor view'>
        <Code />
      </TabsTrigger>
      <TabsTrigger value={WorkflowViewMode.Form} aria-label='Form view'>
        <FileText />
      </TabsTrigger>
    </TabsList>
  </Tabs>
);

export default ModeSwitcher;
