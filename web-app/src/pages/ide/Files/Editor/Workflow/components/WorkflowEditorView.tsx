import { AlertCircle } from "lucide-react";
import type { JSX } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import EditorPageWrapper from "../../components/EditorPageWrapper";
import ModeSwitcher from "./ModeSwitcher";
import type { WorkflowViewMode } from "./types";

interface WorkflowEditorViewProps {
  viewMode: WorkflowViewMode;
  onViewModeChange: (mode: WorkflowViewMode) => void;
  workflowPath: string;
  validationError: string | null;
  pathb64: string;
  isReadOnly: boolean;
  onSaved: () => void;
  customEditor?: JSX.Element;
  gitEnabled: boolean;
  onChanged: (value: string) => void;
  preview?: JSX.Element;
}

const WorkflowEditorView = ({
  viewMode,
  onViewModeChange,
  workflowPath,
  validationError,
  pathb64,
  isReadOnly,
  onSaved,
  customEditor,
  gitEnabled,
  onChanged,
  preview
}: WorkflowEditorViewProps) => {
  return (
    <div className='fade-in flex h-full animate-in flex-col duration-200'>
      <div className='flex items-center gap-2 border-b bg-editor-background px-3 py-1'>
        <ModeSwitcher viewMode={viewMode} onViewModeChange={onViewModeChange} />
        <div className='flex-1 font-medium text-muted-foreground text-sm'>{workflowPath}</div>
        {validationError && (
          <Tooltip>
            <TooltipTrigger asChild>
              <AlertCircle className='h-4 w-4 cursor-pointer text-destructive' />
            </TooltipTrigger>
            <TooltipContent className='max-w-md'>
              <p className='text-sm'>{validationError}</p>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
      <div className='flex-1 overflow-hidden'>
        <EditorPageWrapper
          headerActions={<></>}
          pathb64={pathb64}
          readOnly={isReadOnly}
          onSaved={onSaved}
          customEditor={customEditor}
          git={gitEnabled}
          onChanged={onChanged}
          preview={preview}
        />
      </div>
    </div>
  );
};

export default WorkflowEditorView;
