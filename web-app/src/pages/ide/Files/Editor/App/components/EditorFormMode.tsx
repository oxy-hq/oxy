import { AlertCircle } from "lucide-react";
import AppPreview from "@/components/AppPreview";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import EditorPageWrapper from "../../components/EditorPageWrapper";
import { AppViewMode } from "../types";
import { AppFormWrapper } from "./AppFormWrapper";

interface EditorFormModeProps {
  modeSwitcher: React.ReactNode;
  appPath: string;
  validationError: string | null;
  pathb64: string;
  handleSaved: () => void;
  isReadOnly: boolean;
  gitEnabled: boolean;
  viewMode: AppViewMode;
  validateContent: (value: string) => void;
  previewKey: string;
}

export const EditorFormMode = ({
  modeSwitcher,
  appPath,
  validationError,
  pathb64,
  handleSaved,
  isReadOnly,
  gitEnabled,
  viewMode,
  validateContent,
  previewKey
}: EditorFormModeProps) => {
  return (
    <div className='fade-in flex h-full animate-in flex-col duration-200'>
      <div className='flex items-center gap-2 border-b bg-editor-background px-3 py-1'>
        {modeSwitcher}
        <div className='flex-1 font-medium text-muted-foreground text-sm'>{appPath}</div>
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
          onSaved={handleSaved}
          readOnly={isReadOnly}
          git={gitEnabled}
          customEditor={viewMode === AppViewMode.Form ? <AppFormWrapper /> : undefined}
          previewOnly={viewMode === AppViewMode.Visualization}
          onChanged={(value) => {
            if (viewMode === AppViewMode.Editor) {
              validateContent(value);
            }
          }}
          preview={
            <div className='flex-1 overflow-hidden'>
              <AppPreview key={previewKey} appPath64={pathb64} />
            </div>
          }
        />
      </div>
    </div>
  );
};
