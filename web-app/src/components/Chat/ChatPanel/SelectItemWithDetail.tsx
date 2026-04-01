import { type ComponentProps, type ReactNode, useState } from "react";

import { SelectItem } from "@/components/ui/shadcn/select";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";

interface SelectItemWithDetailProps extends ComponentProps<typeof SelectItem> {
  detail: {
    title: string;
    description: string;
  };
  children: ReactNode;
}

function SelectItemWithDetail({ children, detail, ...props }: SelectItemWithDetailProps) {
  const [open, setOpen] = useState(false);

  return (
    <Tooltip open={open}>
      <TooltipTrigger asChild>
        <SelectItem
          {...props}
          onMouseEnter={() => setOpen(true)}
          onMouseLeave={() => setOpen(false)}
        >
          {children}
        </SelectItem>
      </TooltipTrigger>
      <TooltipContent
        side='right'
        className='max-w-56 bg-card p-3'
        arrowClassName='bg-card fill-card'
        onPointerDownOutside={() => setOpen(false)}
      >
        <p className='font-medium text-card-foreground text-sm'>{detail.title}</p>
        <p className='mt-1 text-muted-foreground text-xs'>{detail.description}</p>
      </TooltipContent>
    </Tooltip>
  );
}

export default SelectItemWithDetail;
