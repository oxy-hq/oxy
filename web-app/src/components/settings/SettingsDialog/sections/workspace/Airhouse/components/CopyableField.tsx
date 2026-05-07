import { Copy } from "lucide-react";
import type React from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";

interface CopyableFieldProps {
  label: string;
  value: string;
  /** When true, render the value with a discoverable but non-fixed-width font. */
  mono?: boolean;
}

export const CopyableField: React.FC<CopyableFieldProps> = ({ label, value, mono = true }) => {
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`Copied ${label}`);
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  };

  return (
    <div className='space-y-1'>
      <p className='text-muted-foreground text-xs'>{label}</p>
      <div className='flex items-center gap-2'>
        <div
          className={
            mono
              ? "flex h-8 flex-1 items-center overflow-x-auto rounded-md border bg-background px-3 font-mono text-sm"
              : "flex h-8 flex-1 items-center overflow-x-auto rounded-md border bg-background px-3 text-sm"
          }
        >
          {value}
        </div>
        <Button variant='outline' size='sm' onClick={handleCopy} aria-label={`Copy ${label}`}>
          <Copy className='h-4 w-4' />
        </Button>
      </div>
    </div>
  );
};
