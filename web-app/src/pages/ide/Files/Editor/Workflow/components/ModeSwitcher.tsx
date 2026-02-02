import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Code, FileText, Play } from "lucide-react";
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
    <TabsList className="h-8">
      <TabsTrigger
        value={WorkflowViewMode.Output}
        className="h-6 px-2"
        aria-label="Output view"
      >
        <Play className="w-4 h-4" />
      </TabsTrigger>
      <TabsTrigger
        value={WorkflowViewMode.Editor}
        className="h-6 px-2"
        aria-label="Editor view"
      >
        <Code className="w-4 h-4" />
      </TabsTrigger>
      <TabsTrigger
        value={WorkflowViewMode.Form}
        className="h-6 px-2"
        aria-label="Form view"
      >
        <FileText className="w-4 h-4" />
      </TabsTrigger>
    </TabsList>
  </Tabs>
);

export default ModeSwitcher;
