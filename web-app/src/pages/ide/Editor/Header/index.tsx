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

interface HeaderProps {
  filePath: string;
  fileState: FileState;
  actions?: React.ReactNode;
}

const Header = ({ filePath, fileState, actions }: HeaderProps) => {
  return (
    <div className={cn("flex justify-between items-center bg-[#1e1e1e] p-2")}>
      <Breadcrumb>
        <BreadcrumbList>
          {filePath.split("/").map((part, index, array) => (
            <>
              <BreadcrumbItem key={index}>
                {index === array.length - 1 ? (
                  <BreadcrumbPage className="text-[#d3d3d3]">
                    {part}
                  </BreadcrumbPage>
                ) : (
                  <BreadcrumbLink className="text-muted-foreground hover:text-[#d3d3d3] truncate">
                    {part}
                  </BreadcrumbLink>
                )}
              </BreadcrumbItem>
              {index < array.length - 1 && <BreadcrumbSeparator />}
              {index === array.length - 1 && (
                <FileStatus fileState={fileState} />
              )}
            </>
          ))}
        </BreadcrumbList>
      </Breadcrumb>
      {actions}
    </div>
  );
};

export default Header;
