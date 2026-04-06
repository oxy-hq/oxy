import { FileDiff, GitBranch } from "lucide-react";
import { useContext } from "react";
import { Fragment } from "react/jsx-runtime";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator
} from "@/components/ui/shadcn/breadcrumb";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { cn } from "@/libs/shadcn/utils";
import { EditorContext } from "@/pages/ide/Files/Editor/contexts/EditorContextTypes";
import { SIDEBAR_REVEAL_FILE } from "@/pages/ide/Files/FilesSidebar";
import FileStatus from "./FileStatus";

interface HeaderProps {
  filePath: string;
  actions?: React.ReactNode;
  prefixAction?: React.ReactNode;
  readOnly?: boolean;
}

const EditorHeader = ({ filePath, actions, prefixAction, readOnly = false }: HeaderProps) => {
  const {
    state: { fileState, git, showDiff },
    actions: fileActions
  } = useFileEditorContext();

  const editorCtx = useContext(EditorContext);
  const isMainEditMode = editorCtx?.isMainEditMode ?? false;

  return (
    <div
      className={cn(
        "flex min-h-[40px] flex-col items-start justify-between gap-4 border-border border-b bg-editor-background px-2 py-1 md:flex-row md:items-center"
      )}
    >
      {prefixAction}
      <div className='flex flex-1 items-center gap-1.5'>
        <Breadcrumb data-testid='ide-breadcrumb'>
          <BreadcrumbList>
            {filePath.split("/").map((part, index, array) => (
              <Fragment key={`${index}-breadcrumb`}>
                <BreadcrumbItem key={index}>
                  {index === array.length - 1 ? (
                    <BreadcrumbPage className='text-foreground'>{part}</BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink
                      className='truncate text-muted-foreground hover:text-foreground'
                      onClick={() => {
                        // reveal this path in the sidebar when breadcrumb clicked
                        const revealPath = array.slice(0, index + 1).join("/");
                        try {
                          window.dispatchEvent(
                            new CustomEvent(SIDEBAR_REVEAL_FILE, {
                              detail: { path: revealPath }
                            })
                          );
                        } catch {
                          // ignore
                        }
                      }}
                    >
                      {part}
                    </BreadcrumbLink>
                  )}
                </BreadcrumbItem>
                {index < array.length - 1 && <BreadcrumbSeparator key={`${index}-separator`} />}
              </Fragment>
            ))}
          </BreadcrumbList>
        </Breadcrumb>
        <FileStatus fileState={fileState} />
      </div>

      <div className='flex items-center gap-1.5'>
        {isMainEditMode && fileState === "modified" && (
          <div className='flex items-center gap-1.5 rounded-md bg-warning/10 px-2 py-1 text-warning text-xs'>
            <GitBranch className='h-3 w-3 flex-shrink-0' />
            <span>Saving will create a new branch</span>
          </div>
        )}
        {fileState === "modified" && !readOnly && (
          <Button
            variant='outline'
            size='sm'
            onClick={() => fileActions.save()}
            data-testid='ide-save-button'
          >
            {isMainEditMode ? "Save to new branch" : "Save changes"}
          </Button>
        )}
        {fileState === "modified" && readOnly && (
          <span className='text-muted-foreground text-sm'>Read-only mode</span>
        )}
        {fileState === "saving" && <Spinner className='text-warning' />}
        {git && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant='outline'
                size='sm'
                onClick={() => fileActions.setShowDiff(!showDiff)}
              >
                <FileDiff className='h-4 w-4' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Show file diff</TooltipContent>
          </Tooltip>
        )}

        {actions}
      </div>
    </div>
  );
};

export default EditorHeader;
