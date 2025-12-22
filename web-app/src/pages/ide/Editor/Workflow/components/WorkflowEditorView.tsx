import { AlertCircle } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/shadcn/tooltip";
import EditorPageWrapper from "../../components/EditorPageWrapper";
import ModeSwitcher from "./ModeSwitcher";
import { WorkflowViewMode } from "./types";
import { JSX } from "react";

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
  preview,
}: WorkflowEditorViewProps) => {
  return (
    <div className="h-full flex flex-col animate-in fade-in duration-200">
      <div className="flex items-center gap-2 px-3 py-1 border-b bg-editor-background">
        <ModeSwitcher viewMode={viewMode} onViewModeChange={onViewModeChange} />
        <div className="text-sm font-medium text-muted-foreground flex-1">
          {workflowPath}
        </div>
        {validationError && (
          <Tooltip>
            <TooltipTrigger asChild>
              <AlertCircle className="w-4 h-4 cursor-pointer text-destructive" />
            </TooltipTrigger>
            <TooltipContent className="max-w-md">
              <p className="text-sm">{validationError}</p>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
      <div className="flex-1 overflow-hidden">
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
