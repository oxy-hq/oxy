import type { ReactNode } from "react";
import { useMemo } from "react";

export function useMentionHighlight(
  message: string,
  mentions: Map<string, string>
): ReactNode | undefined {
  return useMemo(() => {
    if (mentions.size === 0) return undefined;
    const escaped = Array.from(mentions.keys())
      .sort((a, b) => b.length - a.length)
      .map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    const regex = new RegExp(`(@(?:${escaped.join("|")}))`, "g");
    return message.split(regex).map((part, i) => {
      const mentionName = part.startsWith("@") ? part.slice(1) : null;
      if (mentionName && mentions.has(mentionName)) {
        return (
          <span key={i} className='text-vis-orange'>
            {part}
          </span>
        );
      }
      return (
        <span key={i} className='text-foreground'>
          {part}
        </span>
      );
    });
  }, [message, mentions]);
}
