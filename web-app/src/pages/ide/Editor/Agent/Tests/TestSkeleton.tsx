import { Skeleton } from "@/components/ui/shadcn/skeleton";

const TestSkeleton = () => {
  return (
    <div className="flex flex-col gap-10">
      {Array.from({ length: 3 }).map((_, index) => (
        <div key={index} className="flex flex-col gap-4">
          <Skeleton className="h-4 max-w-[200px]" />
          <Skeleton className="h-4 max-w-[500px]" />
          <Skeleton className="h-4 max-w-[500px]" />
        </div>
      ))}
    </div>
  );
};

export default TestSkeleton;
