import { useEffect, useRef, useState } from "react";
import type { Message } from "@/types/chat";

const SCROLL_THRESHOLD = 100;

interface UseSmartScrollOptions {
  messages: Message[];
  enabled?: boolean;
}
interface UseSmartScrollReturn {
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  bottomRef: React.RefObject<HTMLDivElement | null>;
  isAtBottom: boolean;
  scrollToBottom: () => void;
}
export function useSmartScroll({
  messages,
  enabled = true
}: UseSmartScrollOptions): UseSmartScrollReturn {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const hasUserScrolledUpRef = useRef(false);

  const streamingMessage = messages.find((message) => message.isStreaming);

  const [isAtBottom, setIsAtBottom] = useState(true);

  const checkAtBottom = () => {
    if (!scrollContainerRef.current) return true;
    const { scrollTop, scrollHeight, clientHeight } = scrollContainerRef.current;
    return scrollHeight - scrollTop - clientHeight < SCROLL_THRESHOLD;
  };

  useEffect(() => {
    if (!enabled) return;
    const scrollContainer = scrollContainerRef.current;
    if (!scrollContainer) return;

    const handleScroll = () => {
      const atBottom = checkAtBottom();
      setIsAtBottom(atBottom);
      if (!atBottom) {
        hasUserScrolledUpRef.current = true;
      }
    };

    scrollContainer.addEventListener("scroll", handleScroll, { passive: true });
    return () => {
      scrollContainer.removeEventListener("scroll", handleScroll);
    };
  }, [enabled, checkAtBottom]);

  const scrollToBottom = () => {
    hasUserScrolledUpRef.current = false;
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    if (!enabled) return;

    if (streamingMessage && !hasUserScrolledUpRef.current) {
      bottomRef.current?.scrollIntoView({ behavior: "instant" });
    }
  }, [streamingMessage, enabled]);

  return {
    scrollContainerRef,
    bottomRef,
    isAtBottom,
    scrollToBottom
  };
}
