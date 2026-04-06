import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { cn } from "@/libs/shadcn/utils";

type LoadingSkeletonVariant = "content" | "page" | "inline";

interface LoadingSkeletonProps {
  variant?: LoadingSkeletonVariant;
  className?: string;
}

/** Reusable block: three animated bars. */
const SkeletonBlock = () => (
  <div className='flex w-full max-w-[500px] flex-col gap-4'>
    <Skeleton className='h-4 max-w-[200px]' />
    <Skeleton className='h-4 max-w-[500px]' />
    <Skeleton className='h-4 max-w-[500px]' />
  </div>
);

/**
 * Unified loading skeleton used across the entire app.
 * Replaces PageSkeleton, ThreadsSkeleton, etc.
 *
 * Variants:
 *  - "content" (default): centered multi-block placeholder for content areas
 *  - "page": full-height with a header bar + content area
 *  - "inline": single compact block, suitable for cards / sidebar items
 */
const LoadingSkeleton = ({ variant = "content", className }: LoadingSkeletonProps) => {
  if (variant === "inline") {
    return (
      <div className={cn("flex flex-col gap-4 p-4", className)}>
        <SkeletonBlock />
      </div>
    );
  }

  if (variant === "page") {
    return (
      <div className={cn("flex h-full flex-col", className)}>
        <div className='flex h-12 items-center gap-4 border-border border-b px-4'>
          <Skeleton className='h-4 w-24' />
          <Skeleton className='h-4 w-24' />
        </div>
        <div className='mx-auto flex w-full max-w-page-content flex-col gap-10 p-4 py-10'>
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className='mx-auto w-full'>
              <SkeletonBlock />
            </div>
          ))}
        </div>
      </div>
    );
  }

  // "content" — the default
  return (
    <div className={cn("w-full p-4", className)}>
      <div className='mx-auto flex max-w-page-content flex-col gap-10 py-10'>
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className='mx-auto w-full'>
            <SkeletonBlock />
          </div>
        ))}
      </div>
    </div>
  );
};

export default LoadingSkeleton;
