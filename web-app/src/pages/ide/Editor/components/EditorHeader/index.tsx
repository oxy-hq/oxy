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

interface HeaderProps {
  filePath: string;
  fileState: FileState;
  actions?: React.ReactNode;
}

const EditorHeader = ({ filePath, fileState, actions }: HeaderProps) => {
  return (
    <div
      className={cn(
        "flex md:flex-row flex-col justify-between md:items-center item-start bg-editor-background p-2",
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

      {actions}
    </div>
  );
};

export default EditorHeader;
