import { ListTodo, MessagesSquare } from "lucide-react";
import { useMediaQuery } from "usehooks-ts";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";

interface Props {
  onSelect: () => void;
  isSelectionMode: boolean;
  onCancel: () => void;
}

const Header = ({ onSelect, isSelectionMode, onCancel }: Props) => {
  const { open } = useSidebar();
  const isMobile = useMediaQuery("(max-width: 767px)");
  return (
    <div className='fw-full relative'>
      {(!open || isMobile) && (
        <div className='absolute top-0 left-0 z-10'>
          <SidebarTrigger />
        </div>
      )}
      <div className='mx-auto w-full max-w-page-content flex-col border-border border-b px-2 pb-2'>
        <div className='mt-12 flex items-center justify-between md:pt-2'>
          <div className='flex items-center gap-[10px]'>
            <MessagesSquare className='h-9 min-h-9 w-9 min-w-9' strokeWidth={1} />
            <h1 className='font-semibold text-3xl'>Threads</h1>
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
