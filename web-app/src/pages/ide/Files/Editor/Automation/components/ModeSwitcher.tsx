import { Code, FileText, Play } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { AutomationViewMode } from "./types";

interface ModeSwitcherProps {
  viewMode: AutomationViewMode;
  onViewModeChange: (mode: AutomationViewMode) => void;
}

const ModeSwitcher = ({ viewMode, onViewModeChange }: ModeSwitcherProps) => (
  <Tabs
    value={viewMode}
    onValueChange={(value: string) => {
      if (Object.values(AutomationViewMode).includes(value as AutomationViewMode)) {
        onViewModeChange(value as AutomationViewMode);
      }
    }}
  >
    <TabsList>
      <TabsTrigger value={AutomationViewMode.Output} aria-label='Output view'>
        <Play />
      </TabsTrigger>
      <TabsTrigger value={AutomationViewMode.Editor} aria-label='Editor view'>
        <Code />
      </TabsTrigger>
      <TabsTrigger value={AutomationViewMode.Form} aria-label='Form view'>
        <FileText />
      </TabsTrigger>
    </TabsList>
  </Tabs>
);

export default ModeSwitcher;
