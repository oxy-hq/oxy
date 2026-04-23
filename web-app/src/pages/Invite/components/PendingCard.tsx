import { Spinner } from "@/components/ui/shadcn/spinner";
import { CenteredLayout } from "./CenteredLayout";

export function PendingCard() {
  return (
    <CenteredLayout>
      <div className='flex flex-col items-center gap-4 text-center'>
        <Spinner className='size-8 text-primary' />
        <p className='text-muted-foreground text-sm'>Accepting your invitation…</p>
      </div>
    </CenteredLayout>
  );
}
