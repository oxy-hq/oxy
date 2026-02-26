import { Database, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";

interface TopicHeaderProps {
  topic: string;
  database?: string;
  isModified?: boolean;
  onRerun?: () => void;
  className?: string;
}

const TopicHeader = ({ topic, database, isModified, onRerun, className }: TopicHeaderProps) => (
  <div className={cn("border-border border-b px-3 py-2", className)}>
    <div className='flex items-center justify-between'>
      <div className='flex flex-col gap-0.5'>
        <div className='flex items-center gap-1.5'>
          <span className='text-muted-foreground text-xs'>Topic</span>
          <span className='font-medium text-sm'>{topic}</span>
        </div>
        {database && (
          <div className='flex items-center gap-1 text-muted-foreground text-xs'>
            <Database className='h-3 w-3' />
            <span>{database}</span>
          </div>
        )}
      </div>
      {isModified && onRerun && (
        <Button
          variant='ghost'
          size='sm'
          className='h-7 gap-1 text-muted-foreground text-xs hover:text-foreground'
          onClick={onRerun}
        >
          <RotateCcw className='h-3 w-3' />
          Re-run
        </Button>
      )}
    </div>
  </div>
);

export default TopicHeader;
