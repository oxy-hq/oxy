import { AlertCircle, Code, FileText } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";

/// Editor/form toggle for YAML-backed files (agentic-analytics, test files).
export enum FileEditorViewMode {
  Editor = "editor",
  Form = "form"
}

interface ViewModeToggleProps {
  viewMode: FileEditorViewMode;
  onViewModeChange: (mode: FileEditorViewMode) => void;
  validationError: string | null;
}

const ViewModeToggle = ({ viewMode, onViewModeChange, validationError }: ViewModeToggleProps) => {
  return (
    <>
      {validationError ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <AlertCircle className='h-4 w-4 cursor-pointer text-destructive' />
          </TooltipTrigger>
          <TooltipContent className='max-w-md'>
            <p className='text-sm'>{validationError}</p>
          </TooltipContent>
        </Tooltip>
      ) : (
        <Tabs
          value={viewMode}
          onValueChange={(value: string) => {
            if (value === FileEditorViewMode.Form || value === FileEditorViewMode.Editor) {
              onViewModeChange(value as FileEditorViewMode);
            }
          }}
        >
          <TabsList>
            <TabsTrigger value='editor' aria-label='Editor view'>
              <Code />
            </TabsTrigger>
            <TabsTrigger value='form' aria-label='Form view'>
              <FileText />
            </TabsTrigger>
          </TabsList>
        </Tabs>
      )}
    </>
  );
};

export default ViewModeToggle;
