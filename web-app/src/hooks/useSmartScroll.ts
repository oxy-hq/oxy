import { useCallback, useEffect, useRef, useState } from "react";
import { Message } from "@/types/chat";

const SCROLL_THRESHOLD = 150; // pixels from bottom
const SCROLL_DEBOUNCE_MS = 150;

interface UseSmartScrollOptions {
  messages: Message[];
  enabled?: boolean;
  onSendMessage?: () => void;
}

interface UseSmartScrollReturn {
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  bottomRef: React.RefObject<HTMLDivElement | null>;
  scrollToBottom: (behavior?: ScrollBehavior) => void;
  onUserSendMessage: () => void;
  showScrollButton: boolean;
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
  onSendMessage,
}: UseSmartScrollOptions): UseSmartScrollReturn {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const isUserScrollingRef = useRef(false);
  const lastMessageCountRef = useRef(0);
  const [showScrollButton, setShowScrollButton] = useState(false);

  const streamingMessage = messages.find((message) => message.isStreaming);

  // Check if user is near bottom of scroll container
  // Note: scrollContainerRef doesn't need to be in deps since refs are stable
  const isNearBottom = useCallback(() => {
    if (!scrollContainerRef.current) return true;
    const { scrollTop, scrollHeight, clientHeight } =
      scrollContainerRef.current;
    return scrollHeight - scrollTop - clientHeight < SCROLL_THRESHOLD;
  }, []);

  // Memoized function to programmatically scroll to bottom
  const scrollToBottom = useCallback((behavior: ScrollBehavior = "smooth") => {
    bottomRef.current?.scrollIntoView({ behavior });
  }, []);

  // Memoized callback for when user sends a message - scrolls to bottom and calls optional callback
  const onUserSendMessage = useCallback(() => {
    onSendMessage?.();
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [onSendMessage]);

  // Track user scroll behavior and update scroll button visibility
  useEffect(() => {
    if (!enabled) return;

    const scrollContainer = scrollContainerRef.current;
    if (!scrollContainer) return;

    let scrollTimeout: NodeJS.Timeout;
    const handleScroll = () => {
      isUserScrollingRef.current = true;

      // Update scroll button visibility based on scroll position
      const isNearBottomNow =
        scrollContainer.scrollHeight -
          scrollContainer.scrollTop -
          scrollContainer.clientHeight <
        SCROLL_THRESHOLD;
      setShowScrollButton(!isNearBottomNow);

      clearTimeout(scrollTimeout);
      scrollTimeout = setTimeout(() => {
        isUserScrollingRef.current = false;
      }, SCROLL_DEBOUNCE_MS);
    };

    // Initial check
    handleScroll();

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
    scrollToBottom,
    onUserSendMessage,
    showScrollButton,
  };
}
