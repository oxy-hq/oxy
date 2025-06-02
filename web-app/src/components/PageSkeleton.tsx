import PageHeader from "@/components/PageHeader";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Separator } from "@/components/ui/shadcn/separator";

const PageSkeleton = () => {
  return (
    <div className="flex flex-col h-full">
      <PageHeader className="border-b-1 border-border items-center">
        <div className="p-2 flex items-center justify-center flex-1 h-full w-full">
          <div className="flex gap-1 items-center text-muted-foreground">
            <Skeleton className="h-4 min-w-24" />
          </div>
          <div className="px-4 h-full flex items-stretch">
            <Separator orientation="vertical" />
          </div>

          <Skeleton className="h-4 min-w-24" />
        </div>
      </PageHeader>

      <div className="flex-1 w-full">
        <div className="flex flex-col gap-10 max-w-page-content mx-auto py-10">
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className="flex flex-col gap-4">
              <Skeleton className="h-4 max-w-[200px]" />
              <Skeleton className="h-4 max-w-[500px]" />
              <Skeleton className="h-4 max-w-[500px]" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

export default PageSkeleton;
