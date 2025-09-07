import React, { useState, forwardRef } from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Button } from "@/components/ui/shadcn/button";
import { Eye, EyeOff, Copy, Check } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

interface SecretInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  showCopyButton?: boolean;
  onCopy?: () => void;
}

const SecretInput = forwardRef<HTMLInputElement, SecretInputProps>(
  ({ className, showCopyButton = false, onCopy, ...props }, ref) => {
    const [showSecret, setShowSecret] = useState(false);
    const [copied, setCopied] = useState(false);

    const handleToggleVisibility = () => {
      setShowSecret(!showSecret);
    };

    const handleCopy = async () => {
      if (props.value && typeof props.value === "string") {
        try {
          await navigator.clipboard.writeText(props.value);
          setCopied(true);
          onCopy?.();
          setTimeout(() => setCopied(false), 2000);
        } catch (err) {
          console.error("Failed to copy to clipboard:", err);
        }
      }
    };

    return (
      <div className="relative">
        <Input
          {...props}
          ref={ref}
          type={showSecret ? "text" : "password"}
          className={cn("pr-20", showCopyButton && "pr-32", className)}
        />
        <div className="absolute inset-y-0 right-0 flex items-center gap-1 pr-1">
          {showCopyButton && props.value && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0"
              onClick={handleCopy}
              title={copied ? "Copied!" : "Copy to clipboard"}
            >
              {copied ? (
                <Check className="h-3 w-3 text-green-600" />
              ) : (
                <Copy className="h-3 w-3" />
              )}
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={handleToggleVisibility}
            title={showSecret ? "Hide secret" : "Show secret"}
          >
            {showSecret ? (
              <EyeOff className="h-3 w-3" />
            ) : (
              <Eye className="h-3 w-3" />
            )}
          </Button>
        </div>
      </div>
    );
  },
);

SecretInput.displayName = "SecretInput";

export { SecretInput };
