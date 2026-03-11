import { AppWindow, Code, FileText } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { AppViewMode } from "../types";

interface ModeSwitcherProps {
  viewMode: AppViewMode;
  setViewMode: (mode: AppViewMode) => void;
}

export const ModeSwitcher = ({ viewMode, setViewMode }: ModeSwitcherProps) => {
  return (
    <Tabs
      value={viewMode}
      onValueChange={(value: string) => {
        if (Object.values(AppViewMode).includes(value as AppViewMode)) {
          setViewMode(value as AppViewMode);
        }
      }}
    >
      <TabsList>
        <TabsTrigger value={AppViewMode.Visualization} aria-label='Visualization view'>
          <AppWindow />
        </TabsTrigger>
        <TabsTrigger value={AppViewMode.Editor} aria-label='Editor view'>
          <Code />
        </TabsTrigger>
        <TabsTrigger value={AppViewMode.Form} aria-label='Form view'>
          <FileText />
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
};
