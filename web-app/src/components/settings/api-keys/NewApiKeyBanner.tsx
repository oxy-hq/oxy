import React, { useState } from "react";
import { Copy, Eye, EyeOff, AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { CreateApiKeyResponse } from "@/types/apiKey";
import { toast } from "sonner";

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
    <div className="mb-6 p-4 border rounded-lg bg-muted/50">
      <div className="flex items-center gap-2 mb-2">
        <AlertTriangle className="w-5 h-5 text-muted-foreground" />
        <h3 className="font-semibold">API Key Created</h3>
      </div>
      <p className="text-sm text-muted-foreground mb-3">
        Please copy your API key now. For security reasons, you won't be able to
        see it again.
      </p>
      <div className="flex items-center gap-2">
        <div className="flex-1 h-8 px-3 bg-background border rounded-md font-mono text-sm flex items-center">
          {isVisible ? apiKey.key : "••••••••••••••••••••••••••••••••"}
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsVisible(!isVisible)}
        >
          {isVisible ? <EyeOff /> : <Eye />}
        </Button>
        <Button variant="outline" size="sm" onClick={copyToClipboard}>
          <Copy />
        </Button>
        <Button variant="outline" size="sm" onClick={onDismiss}>
          Dismiss
        </Button>
      </div>
    </div>
  );
};

export default NewApiKeyBanner;
