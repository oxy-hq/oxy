import { Button } from "@/components/ui/shadcn/button";
import { SidebarTrigger, useSidebar } from "@/components/ui/shadcn/sidebar";
import { ListTodo, MessagesSquare } from "lucide-react";
import { useMediaQuery } from "usehooks-ts";

interface Props {
  onSelect: () => void;
  isSelectionMode: boolean;
  onCancel: () => void;
}

const Header = ({ onSelect, isSelectionMode, onCancel }: Props) => {
  const { open } = useSidebar();
  const isMobile = useMediaQuery("(max-width: 767px)");
  return (
    <div className="fw-full relative">
      {(!open || isMobile) && (
        <div className="absolute top-0 left-0 z-10">
          <SidebarTrigger />
        </div>
      )}
      <div className="flex-col border-b border-border w-full max-w-page-content mx-auto pb-2 px-2">
        <div className="flex justify-between items-center md:pt-2 mt-12">
          <div className="flex gap-[10px] items-center">
            <MessagesSquare
              className="w-9 h-9 min-w-9 min-h-9"
              strokeWidth={1}
            />
            <h1 className="text-3xl font-semibold">Threads</h1>
          </div>
          {isSelectionMode ? (
            <Button variant="secondary" onClick={onCancel}>
              Cancel
            </Button>
          ) : (
            <Button variant="outline" onClick={onSelect}>
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
