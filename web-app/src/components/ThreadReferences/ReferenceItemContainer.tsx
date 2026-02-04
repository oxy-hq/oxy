import { cn } from "@/libs/shadcn/utils";

type ReferenceItemContainerProps = {
  children: React.ReactNode;
  isOpen: boolean;
};

export const ReferenceItemContainer = ({ children, isOpen }: ReferenceItemContainerProps) => {
  return (
    <div
      className={cn("h-21 rounded-md border bg-sidebar-accent hover:bg-input", {
        "bg-input": isOpen
      })}
    >
      {children}
    </div>
  );
};
