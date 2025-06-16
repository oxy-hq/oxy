import React, { useState } from "react";
import { Copy, Eye, EyeOff, AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { CreateApiKeyResponse } from "@/types/apiKey";

interface NewApiKeyBannerProps {
  apiKey: CreateApiKeyResponse;
  onDismiss: () => void;
  onCopy: (text: string) => void;
}

export const NewApiKeyBanner: React.FC<NewApiKeyBannerProps> = ({
  apiKey,
  onDismiss,
  onCopy,
}) => {
  const [isVisible, setIsVisible] = useState(false);

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
        <div className="flex-1 p-3 bg-background border rounded font-mono text-sm">
          {isVisible ? apiKey.key : "••••••••••••••••••••••••••••••••"}
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsVisible(!isVisible)}
        >
          {isVisible ? (
            <EyeOff className="w-4 h-4" />
          ) : (
            <Eye className="w-4 h-4" />
          )}
        </Button>
        <Button variant="outline" size="sm" onClick={() => onCopy(apiKey.key)}>
          <Copy className="w-4 h-4" />
        </Button>
        <Button variant="outline" size="sm" onClick={onDismiss}>
          Dismiss
        </Button>
      </div>
    </div>
  );
};
