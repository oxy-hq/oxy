import { Code, View } from "lucide-react";
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
    <TabsList className='h-8'>
      <TabsTrigger value={ViewMode.Explorer} className='h-6 px-2' aria-label='Explorer view'>
        <View className='h-4 w-4' />
      </TabsTrigger>
      <TabsTrigger value={ViewMode.Editor} className='h-6 px-2' aria-label='Editor view'>
        <Code className='h-4 w-4' />
      </TabsTrigger>
    </TabsList>
  </Tabs>
);

export default ModeSwitcher;
