import { Code, Eye } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { ViewMode } from "./types";

interface ModeSwitcherProps {
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
}

const ModeSwitcher = ({ viewMode, onViewModeChange }: ModeSwitcherProps) => (
  <Tabs
    value={viewMode}
    onValueChange={(value: string) => {
      if (Object.values(ViewMode).includes(value as ViewMode)) {
        onViewModeChange(value as ViewMode);
      }
    }}
  >
    <TabsList>
      <TabsTrigger value={ViewMode.Explorer} aria-label='Explorer view'>
        <Eye />
      </TabsTrigger>
      <TabsTrigger value={ViewMode.Editor} aria-label='Editor view'>
        <Code />
      </TabsTrigger>
    </TabsList>
  </Tabs>
);

export default ModeSwitcher;
