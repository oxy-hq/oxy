import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/shadcn/breadcrumb";
import FileStatus from "./FileStatus";
import { cn } from "@/libs/shadcn/utils";
import { Fragment } from "react/jsx-runtime";
import { FileDiff, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { SIDEBAR_REVEAL_FILE } from "@/pages/ide/Files/FilesSidebar";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";

interface HeaderProps {
  filePath: string;
  actions?: React.ReactNode;
  readOnly?: boolean;
}

const EditorHeader = ({ filePath, actions, readOnly = false }: HeaderProps) => {
  const {
    state: { fileState, git, showDiff },
    actions: fileActions,
  } = useFileEditorContext();
  return (
    <div
      className={cn(
        // keep header visually above Monaco editor and add spacing so buttons don't get overlapped
        "flex md:flex-row flex-col justify-between md:items-center items-start bg-editor-background px-2 py-1 min-h-[40px] relative z-10",
      )}
    >
      <div className="flex gap-1.5 items-center">
        <Breadcrumb data-testid="ide-breadcrumb">
          <BreadcrumbList>
            {filePath.split("/").map((part, index, array) => (
              <Fragment key={`${index}-breadcrumb`}>
                <BreadcrumbItem key={index}>
                  {index === array.length - 1 ? (
                    <BreadcrumbPage className="text-foreground">
                      {part}
                    </BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink
                      className="text-muted-foreground hover:text-foreground truncate"
                      onClick={() => {
                        // reveal this path in the sidebar when breadcrumb clicked
                        const revealPath = array.slice(0, index + 1).join("/");
                        try {
                          window.dispatchEvent(
                            new CustomEvent(SIDEBAR_REVEAL_FILE, {
                              detail: { path: revealPath },
                            }),
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
                {index < array.length - 1 && (
                  <BreadcrumbSeparator key={`${index}-separator`} />
                )}
              </Fragment>
            ))}
          </BreadcrumbList>
        </Breadcrumb>
        <FileStatus fileState={fileState} />
      </div>

      <div className="flex gap-1.5 items-center">
        {fileState == "modified" && !readOnly && (
          <Button
            variant="secondary"
            size="sm"
            className="text-foreground hover:text-secondary-foreground"
            onClick={() => fileActions.save()}
            data-testid="ide-save-button"
          >
            Save changes
          </Button>
        )}
        {fileState == "modified" && readOnly && (
          <span className="text-sm text-muted-foreground">Read-only mode</span>
        )}
        {fileState == "saving" && (
          <Loader2 className="w-4 h-4 text-yellow-500 animate-[spin_0.2s_linear_infinite]" />
        )}
        {git && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="sm"
                onClick={() => fileActions.setShowDiff(!showDiff)}
              >
                <FileDiff className="w-4 h-4" />
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
