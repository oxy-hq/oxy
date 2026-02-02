import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Code, FileText, AppWindow } from "lucide-react";
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
      <TabsList className="h-8">
        <TabsTrigger
          value={AppViewMode.Visualization}
          className="h-6 px-2"
          aria-label="Visualization view"
        >
          <AppWindow className="w-4 h-4" />
        </TabsTrigger>
        <TabsTrigger
          value={AppViewMode.Editor}
          className="h-6 px-2"
          aria-label="Editor view"
        >
          <Code className="w-4 h-4" />
        </TabsTrigger>
        <TabsTrigger
          value={AppViewMode.Form}
          className="h-6 px-2"
          aria-label="Form view"
        >
          <FileText className="w-4 h-4" />
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
};
