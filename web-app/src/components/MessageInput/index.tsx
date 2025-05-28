import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { ArrowRight, Loader2 } from "lucide-react";
import { useRef } from "react";

interface MessageInputProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  disabled: boolean;
  isLoading?: boolean;
  showWarning?: boolean;
}

const MessageInput = ({
  value,
  onChange,
  onSend,
  disabled,
  isLoading = false,
  showWarning = false,
}: MessageInputProps) => {
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      onSend();
    }
  };

  return (
    <div className="flex flex-col gap-1 w-full flex-1">
      {showWarning && (
        <div className="w-full p-2 bg-gray-900 border border-gray-950 rounded-lg">
          <div className="flex items-center">
            <svg
              className="w-5 h-5 text-amber-500 mr-2"
              fill="currentColor"
              viewBox="0 0 20 20"
            >
              <path
                fillRule="evenodd"
                d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z"
                clipRule="evenodd"
              />
            </svg>
            <span className="text-white text-sm font-medium">
              You've asked a lot of questions. You may want to start a new
              thread for optimal performance.
            </span>
          </div>
        </div>
      )}
      <div>
        <div className="relative">
          <Textarea
            ref={inputRef}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder="Ask a follow-up question..."
            className="pr-12 resize-none min-h-[60px] border-neutral-700 bg-secondary"
            disabled={disabled}
            onKeyDown={handleKeyDown}
          />
          <Button
            size="icon"
            className="absolute right-2 transform top-1/2 -translate-y-1/2"
            onClick={onSend}
            disabled={!value.trim() || disabled}
          >
            {isLoading ? (
              <Loader2 className="animate-spin h-5 w-5" />
            ) : (
              <ArrowRight className="h-5 w-5" />
            )}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default MessageInput;
