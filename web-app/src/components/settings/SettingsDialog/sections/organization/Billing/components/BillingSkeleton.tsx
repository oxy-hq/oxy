import { Skeleton } from "@/components/ui/shadcn/skeleton";

export function BillingSkeleton() {
  return (
    <div className='space-y-8'>
      <section className='flex items-start justify-between gap-4'>
        <div className='flex items-start gap-3'>
          <Skeleton className='mt-1 size-7 rounded-md' />
          <div className='space-y-2'>
            <Skeleton className='h-5 w-48' />
            <Skeleton className='h-4 w-64' />
            <Skeleton className='h-3 w-56' />
          </div>
        </div>
        <Skeleton className='h-9 w-44' />
      </section>
      <section className='space-y-3'>
        <Skeleton className='h-14 w-full' />
        <Skeleton className='h-14 w-full' />
      </section>
    </div>
  );
}
