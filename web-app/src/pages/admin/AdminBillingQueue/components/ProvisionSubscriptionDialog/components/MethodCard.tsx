import { Info } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Card } from "@/components/ui/shadcn/card";
import { cn } from "@/libs/shadcn/utils";

interface MethodCardProps {
  title: string;
  description: string;
  note: string;
  recommended?: boolean;
  selected: boolean;
  onSelect: () => void;
  onShowDetail: () => void;
}

export function MethodCard({
  title,
  description,
  note,
  recommended,
  selected,
  onSelect,
  onShowDetail
}: MethodCardProps) {
  return (
    <Card
      role='button'
      tabIndex={0}
      aria-pressed={selected}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      className={cn(
        "flex-1 cursor-pointer gap-2 p-3 transition-colors",
        selected ? "border-primary ring-1 ring-primary" : "hover:bg-muted/40"
      )}
    >
      <div className='flex items-start justify-between gap-2'>
        <div className='flex flex-wrap items-center gap-1.5'>
          <span className='font-medium text-sm'>{title}</span>
          {recommended ? (
            <Badge variant='secondary' className='text-[10px]'>
              Recommended
            </Badge>
          ) : null}
        </div>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          onClick={(e) => {
            e.stopPropagation();
            onShowDetail();
          }}
          aria-label={`View ${title} details`}
          className='-mt-1 -mr-1 size-6 shrink-0'
        >
          <Info className='size-3.5' />
        </Button>
      </div>
      <p className='text-muted-foreground text-xs leading-relaxed'>{description}</p>
      <p className='border-t pt-2 text-muted-foreground/80 text-xs italic leading-relaxed'>
        {note}
      </p>
    </Card>
  );
}
