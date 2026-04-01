import { AlertCircle, Code, FileText } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { AgentViewMode } from "../types";

interface ViewModeToggleProps {
  viewMode: AgentViewMode;
  onViewModeChange: (mode: AgentViewMode) => void;
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
            if (value === AgentViewMode.Form || value === AgentViewMode.Editor) {
              onViewModeChange(value as AgentViewMode);
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
