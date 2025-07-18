import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/shadcn/breadcrumb";
import { FileState } from "@/components/FileEditor";
import FileStatus from "./FileStatus";
import { cn } from "@/libs/shadcn/utils";
import { Fragment } from "react/jsx-runtime";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface HeaderProps {
  filePath: string;
  fileState: FileState;
  actions?: React.ReactNode;
  onSave: () => void;
  isReadonly?: boolean;
}

const EditorHeader = ({
  filePath,
  fileState,
  actions,
  onSave,
  isReadonly = false,
}: HeaderProps) => {
  return (
    <div
      className={cn(
        "flex md:flex-row flex-col justify-between md:items-center item-start bg-editor-background p-2 min-h-[64px]",
      )}
    >
      <div className="flex gap-1.5 items-center">
        <Breadcrumb>
          <BreadcrumbList>
            {filePath.split("/").map((part, index, array) => (
              <Fragment key={`${index}-breadcrumb`}>
                <BreadcrumbItem key={index}>
                  {index === array.length - 1 ? (
                    <BreadcrumbPage className="text-foreground">
                      {part}
                    </BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink className="text-muted-foreground hover:text-foreground truncate">
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

      <div className="flex gap-2 items-center p-2">
        {fileState == "modified" && !isReadonly && (
          <Button
            variant="secondary"
            size="sm"
            className="text-foreground hover:text-secondary-foreground"
            onClick={onSave}
          >
            Save changes
          </Button>
        )}
        {fileState == "modified" && isReadonly && (
          <span className="text-sm text-muted-foreground">Read-only mode</span>
        )}
        {fileState == "saving" && (
          <Loader2 className="w-4 h-4 text-yellow-500 animate-[spin_0.2s_linear_infinite]" />
        )}
        {actions}
      </div>
    </div>
  );
};

export default EditorHeader;
