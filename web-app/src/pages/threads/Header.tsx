import { ListTodo, MessagesSquare } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { cn } from "@/libs/shadcn/utils";

interface Props {
  onSelect: () => void;
  isSelectionMode: boolean;
  onCancel: () => void;
}

const Header = ({ onSelect, isSelectionMode, onCancel }: Props) => {
  const { open, isMobile } = useSidebar();
  const showTrigger = !open || isMobile;
  return (
    <div className='fw-full relative'>
      {showTrigger && (
        <div className='absolute top-0 left-0 z-10'>
          <SidebarTrigger />
        </div>
      )}
      <div className='mx-auto w-full max-w-page-content flex-col border-border border-b px-2 pb-2'>
        <div
          className={cn(
            "flex items-center justify-between md:pt-2",
            showTrigger ? "mt-12" : "mt-0"
          )}
        >
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
