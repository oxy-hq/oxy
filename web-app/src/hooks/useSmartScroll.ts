import { useCallback, useEffect, useRef } from "react";
import { Message } from "@/types/chat";

const SCROLL_THRESHOLD = 150; // pixels from bottom
const SCROLL_DEBOUNCE_MS = 150;

interface UseSmartScrollOptions {
  messages: Message[];
  enabled?: boolean;
}

interface UseSmartScrollReturn {
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  bottomRef: React.RefObject<HTMLDivElement | null>;
}

/**
 * Custom hook for smart auto-scrolling behavior in message threads.
 *
 * Features:
 * - Auto-scrolls to bottom for new messages
 * - Respects user scroll position during streaming
 * - Only auto-scrolls during streaming if user is near bottom
 * - Prevents interruption when user scrolls up to review history
 */
export function useSmartScroll({
  messages,
  enabled = true,
}: UseSmartScrollOptions): UseSmartScrollReturn {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const isUserScrollingRef = useRef(false);
  const lastMessageCountRef = useRef(0);

  const streamingMessage = messages.find((message) => message.isStreaming);

  // Check if user is near bottom of scroll container
  const isNearBottom = useCallback(() => {
    if (!scrollContainerRef.current) return true;
    const { scrollTop, scrollHeight, clientHeight } =
      scrollContainerRef.current;
    return scrollHeight - scrollTop - clientHeight < SCROLL_THRESHOLD;
  }, []);

  // Track user scroll behavior
  useEffect(() => {
    if (!enabled) return;

    const scrollContainer = scrollContainerRef.current;
    if (!scrollContainer) return;

    let scrollTimeout: NodeJS.Timeout;
    const handleScroll = () => {
      isUserScrollingRef.current = true;
      clearTimeout(scrollTimeout);
      scrollTimeout = setTimeout(() => {
        isUserScrollingRef.current = false;
      }, SCROLL_DEBOUNCE_MS);
    };

    scrollContainer.addEventListener("scroll", handleScroll, { passive: true });
    return () => {
      scrollContainer.removeEventListener("scroll", handleScroll);
      clearTimeout(scrollTimeout);
    };
  }, [enabled]);

  // Auto-scroll only if user is near bottom or new message arrives
  useEffect(() => {
    if (!enabled) return;

    const isNewMessage = messages.length > lastMessageCountRef.current;
    lastMessageCountRef.current = messages.length;

    // Always scroll for new messages (not streaming updates)
    if (isNewMessage && !streamingMessage) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
      return;
    }

    // For streaming updates, only scroll if user is near bottom
    if (streamingMessage && !isUserScrollingRef.current && isNearBottom()) {
      bottomRef.current?.scrollIntoView({ behavior: "instant" });
    }
  }, [messages, streamingMessage, isNearBottom, enabled]);

  return {
    scrollContainerRef,
    bottomRef,
  };
}
