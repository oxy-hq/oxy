import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { InvoicesTableHeader } from "./InvoicesTableHeader";

export function InvoicesSkeleton() {
  return (
    <table className='w-full text-sm'>
      <InvoicesTableHeader />
      <tbody>
        {Array.from({ length: 3 }).map((_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static skeleton placeholders
          <tr key={`skeleton-${i}`} className='border-b last:border-0'>
            <td className='px-3 py-3'>
              <Skeleton className='h-4 w-24' />
            </td>
            <td className='px-3 py-3'>
              <Skeleton className='h-4 w-24' />
            </td>
            <td className='px-3 py-3'>
              <Skeleton className='ml-auto h-4 w-16' />
            </td>
            <td className='px-3 py-3'>
              <Skeleton className='h-4 w-16' />
            </td>
            <td className='px-3 py-3'>
              <Skeleton className='ml-auto h-4 w-10' />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
