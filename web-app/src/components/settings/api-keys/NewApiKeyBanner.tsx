import { AlertTriangle, Copy, Eye, EyeOff } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import type { CreateApiKeyResponse } from "@/types/apiKey";

interface Props {
  apiKey: CreateApiKeyResponse;
  onDismiss: () => void;
}

const NewApiKeyBanner: React.FC<Props> = ({ apiKey, onDismiss }) => {
  const [isVisible, setIsVisible] = useState(false);

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(apiKey.key);
      toast.success("Copied to clipboard");
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
      toast.error("Failed to copy to clipboard");
    }
  };

  return (
    <div className='mb-6 rounded-lg border bg-muted/50 p-4'>
      <div className='mb-2 flex items-center gap-2'>
        <AlertTriangle className='h-5 w-5 text-muted-foreground' />
        <h3 className='font-semibold'>API Key Created</h3>
      </div>
      <p className='mb-3 text-muted-foreground text-sm'>
        Please copy your API key now. For security reasons, you won't be able to see it again.
      </p>
      <div className='flex items-center gap-2'>
        <div className='flex h-8 flex-1 items-center rounded-md border bg-background px-3 font-mono text-sm'>
          {isVisible ? apiKey.key : "••••••••••••••••••••••••••••••••"}
        </div>
        <Button variant='outline' size='sm' onClick={() => setIsVisible(!isVisible)}>
          {isVisible ? <EyeOff /> : <Eye />}
        </Button>
        <Button variant='outline' size='sm' onClick={copyToClipboard}>
          <Copy />
        </Button>
        <Button variant='outline' size='sm' onClick={onDismiss}>
          Dismiss
        </Button>
      </div>
    </div>
  );
};

export default NewApiKeyBanner;
