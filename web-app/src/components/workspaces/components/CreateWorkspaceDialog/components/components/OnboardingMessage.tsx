import { Check, Clock, Loader2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { cn } from "@/libs/shadcn/utils";
import type { OnboardingMessage as MessageType } from "../types";
import TypewriterText from "./TypewriterText";

interface OnboardingMessageProps {
  message: MessageType;
  children?: React.ReactNode;
  /** Whether this is the latest message (enables typewriter) */
  isLatest?: boolean;
}

export default function OnboardingMessage({
  message,
  children,
  isLatest = false
}: OnboardingMessageProps) {
  const [visible, setVisible] = useState(!isLatest);
  const [typewriterDone, setTypewriterDone] = useState(!isLatest || !message.content);

  const shouldTypewrite =
    isLatest && message.role === "assistant" && !message.status && message.status !== "pending";

  // Fade in with a short delay for new messages.
  // Also handles the case where a message was initially the latest (visible=false)
  // but then new messages were added, making it no longer the latest.
  useEffect(() => {
    if (!visible) {
      const timer = setTimeout(() => setVisible(true), isLatest ? 150 : 0);
      return () => clearTimeout(timer);
    }
  }, [isLatest, visible]);

  // When the typewriter is skipped (message has a status), ensure children are visible
  useEffect(() => {
    if (!shouldTypewrite && !typewriterDone) {
      setTypewriterDone(true);
    }
  }, [shouldTypewrite, typewriterDone]);

  if (message.role === "user") {
    return (
      <div
        className={cn(
          "flex justify-end transition-all duration-300",
          visible ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0"
        )}
      >
        <div className='max-w-[80%] rounded-2xl bg-primary/10 px-4 py-2.5 text-sm'>
          {message.content}
        </div>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex flex-col gap-3 transition-all duration-300",
        visible ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0"
      )}
    >
      <div className='flex items-start gap-3'>
        <StatusIndicator status={message.status} />
        <div className='flex-1'>
          {message.content && (
            <p
              className={cn(
                "text-sm leading-relaxed",
                message.status === "complete" && "text-muted-foreground",
                message.status === "pending" && "text-muted-foreground/60",
                message.status === "error" && "text-destructive"
              )}
            >
              {shouldTypewrite ? (
                <TypewriterText text={message.content} onComplete={() => setTypewriterDone(true)} />
              ) : (
                message.content
              )}
            </p>
          )}
        </div>
      </div>

      {/* Show children (input blocks) after typewriter completes */}
      {children && (
        <div
          className={cn(
            "ml-8 transition-all duration-300",
            typewriterDone ? "translate-y-0 opacity-100" : "translate-y-1 opacity-0"
          )}
        >
          {children}
        </div>
      )}
    </div>
  );
}

function StatusIndicator({ status }: { status?: "working" | "complete" | "error" | "pending" }) {
  if (status === "pending") {
    return (
      <div className='mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center'>
        <Clock className='h-3.5 w-3.5 text-muted-foreground/40' />
      </div>
    );
  }
  if (status === "working") {
    return (
      <div className='mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin text-primary' />
      </div>
    );
  }
  if (status === "complete") {
    return (
      <div className='mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/10'>
        <Check className='h-3 w-3 text-primary' />
      </div>
    );
  }
  if (status === "error") {
    return (
      <div className='mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-destructive/10'>
        <X className='h-3 w-3 text-destructive' />
      </div>
    );
  }
  // Default: "oxy" dot
  return (
    <div className='mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center'>
      <div className='h-2 w-2 rounded-full bg-primary' />
    </div>
  );
}
