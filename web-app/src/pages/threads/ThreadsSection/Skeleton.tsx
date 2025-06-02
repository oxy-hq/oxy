import { Skeleton } from "@/components/ui/shadcn/skeleton";

const ThreadsSkeleton = () => {
  return (
    <div className="flex flex-col gap-10">
      {Array.from({ length: 6 }).map((_, index) => (
        <div key={index} className="flex flex-col gap-4">
          <Skeleton className="h-6 max-w-[120px]" />
          <Skeleton className="h-7 max-w-[400px]" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 max-w-[300px]" />
          </div>
        </div>
      ))}
    </div>
  );
};
export default ThreadsSkeleton;
