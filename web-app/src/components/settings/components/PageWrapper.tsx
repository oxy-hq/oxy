import { ChevronLeft, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface Props {
  children: React.ReactNode;
  actions?: React.ReactNode;
  title: string;
  loading?: boolean;
  onBack?: () => void;
}

const PageWrapper = ({ children, title, actions, loading, onBack }: Props) => {
  return (
    <div className='space-y-6 p-4'>
      <div className='flex items-center justify-between border-border border-b pb-2'>
        <div className='flex items-center gap-2'>
          {onBack && (
            <Button variant='ghost' size='icon' onClick={onBack} className='h-9 w-9'>
              <ChevronLeft className='h-5 w-5' />
            </Button>
          )}
          <h3 className='h-9 content-center text-xl'>{title}</h3>
        </div>

        {!loading && actions}
      </div>
      <div>
        {!loading ? (
          children
        ) : (
          <div className='flex h-30 items-center justify-center'>
            <Loader2 className='h-6 w-6 animate-spin' />
          </div>
        )}
      </div>
    </div>
  );
};

export default PageWrapper;
