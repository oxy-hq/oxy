import { Check, Copy, Maximize2, Minimize2, X } from "lucide-react";
import type React from "react";
import { Button } from "@/components/ui/shadcn/button";
import type { LogItem } from "@/services/types";
import { useCopyTimeout } from "./useCopyTimeout";

interface HeaderProps {
  toggleOutput: () => void;
  logs?: LogItem[];
  onExpandAll?: () => void;
  onCollapseAll?: () => void;
}

const getAllContent = (items: LogItem[]): string => {
  let content = "";
  items.forEach((item) => {
    // Only include content from leaf nodes (items without children)
    // This excludes step headers like "workflow started" or "Task started"
    if (!item.children || item.children.length === 0) {
      content += `${item.content}\n\n`;
    } else {
      // Recursively process children
      content += getAllContent(item.children);
    }
  });
  return content.trim();
};

const Header: React.FC<HeaderProps> = ({ toggleOutput, logs = [], onExpandAll, onCollapseAll }) => {
  const { copied, handleCopy } = useCopyTimeout();

  const handleCopyAll = async () => {
    const allContent = getAllContent(logs);
    await handleCopy(allContent);
  };

  return (
    <div className='flex items-center justify-between border border-border bg-card px-2 py-1'>
      <span className='text-background-foreground text-sm'>Output</span>
      <div className='flex items-center gap-1'>
        {logs.length > 0 && (
          <>
            <Button
              variant='ghost'
              content='icon'
              onClick={onExpandAll}
              title='Expand all'
              aria-label='Expand all'
            >
              <Maximize2 size={14} />
            </Button>
            <Button
              variant='ghost'
              content='icon'
              onClick={onCollapseAll}
              title='Collapse all'
              aria-label='Collapse all'
            >
              <Minimize2 size={14} />
            </Button>
            <Button
              variant='ghost'
              content='icon'
              onClick={handleCopyAll}
              title='Copy all outputs'
              aria-label='Copy all outputs'
            >
              {copied ? <Check size={14} className='text-green-500' /> : <Copy size={14} />}
            </Button>
          </>
        )}
        <Button
          variant='ghost'
          content='icon'
          onClick={toggleOutput}
          aria-label='Close output panel'
        >
          <X size={14} />
        </Button>
      </div>
    </div>
  );
};

export default Header;
