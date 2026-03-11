import { Skeleton } from "@/components/ui/shadcn/skeleton";

export const ContentSkeleton = () => (
  <div className='w-full p-4'>
    <div className='mx-auto flex max-w-page-content flex-col gap-10 py-10'>
      {Array.from({ length: 3 }).map((_, index) => (
        <div key={index} className='mx-auto flex w-full max-w-[500px] flex-col gap-4'>
          <Skeleton className='h-4 max-w-[200px]' />
          <Skeleton className='h-4 max-w-[500px]' />
          <Skeleton className='h-4 max-w-[500px]' />
        </div>
      ))}
    </div>
  </div>
);
