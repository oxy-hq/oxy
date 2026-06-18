import { ListTodo, MessagesSquare } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface Props {
  onSelect: () => void;
  isSelectionMode: boolean;
  onCancel: () => void;
}

const Header = ({ onSelect, isSelectionMode, onCancel }: Props) => {
  return (
    <div className='fw-full'>
      <div className='mx-auto w-full max-w-page-content flex-col border-border border-b px-2 pb-2'>
        <div className='flex items-center justify-between md:pt-2'>
          <div className='flex items-center gap-[10px]'>
            <MessagesSquare className='h-9 min-h-9 w-9 min-w-9' strokeWidth={1} />
            <h1 className='font-semibold text-2xl sm:text-3xl'>Threads</h1>
          </div>
          {isSelectionMode ? (
            <Button variant='secondary' onClick={onCancel}>
              Cancel
            </Button>
          ) : (
            <Button variant='outline' onClick={onSelect}>
              <ListTodo />
              Select
            </Button>
          )}
        </div>
      </div>
    </div>
  );
};

export default Header;
