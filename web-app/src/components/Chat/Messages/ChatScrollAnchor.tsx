import { useInView } from "react-intersection-observer";

interface ChatScrollAnchorProps {
  trackVisibility?: boolean;
}

export function ChatScrollAnchor({ trackVisibility }: ChatScrollAnchorProps) {
  const { ref, entry, inView } = useInView({
    trackVisibility,
    delay: 100,
    rootMargin: "0px 0px 50px 0px",
  });

  if (trackVisibility && !inView) {
    entry?.target.scrollIntoView({
      block: "end",
    });
  }

  return <div ref={ref} />;
}
