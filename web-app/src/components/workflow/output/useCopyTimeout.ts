import { useState, useRef, useEffect, useCallback } from "react";

export function useCopyTimeout() {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Cleanup timeout on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  const handleCopy = useCallback(async (content: string) => {
    try {
      // Clear any existing timeout
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }

      await navigator.clipboard.writeText(content);
      setCopied(true);

      timeoutRef.current = setTimeout(() => {
        setCopied(false);
        timeoutRef.current = null;
      }, 2000);

      return true;
    } catch (err) {
      console.error("Failed to copy:", err);
      return false;
    }
  }, []);

  return { copied, handleCopy };
}
