import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import { Skeleton } from "@/components/ui/shadcn/skeleton";

const PageSkeleton = () => {
  return (
    <div className='flex h-full flex-col'>
      <PageHeader className='items-center border-border border-b-1'>
        <div className='flex h-full w-full flex-1 items-center justify-center p-2'>
          <div className='flex items-center gap-1 text-muted-foreground'>
            <Skeleton className='h-4 min-w-24' />
          </div>
          <div className='flex h-full items-stretch px-4'>
            <Separator orientation='vertical' />
          </div>

          <Skeleton className='h-4 min-w-24' />
        </div>
      </PageHeader>

      <div className='w-full flex-1'>
        <div className='mx-auto flex max-w-page-content flex-col gap-10 py-10'>
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className='flex flex-col gap-4'>
              <Skeleton className='h-4 max-w-[200px]' />
              <Skeleton className='h-4 max-w-[500px]' />
              <Skeleton className='h-4 max-w-[500px]' />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

export default PageSkeleton;
