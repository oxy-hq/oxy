import { useEffect, useRef, useState } from "react";

interface TypewriterTextProps {
  text: string;
  onComplete?: () => void;
  enabled?: boolean;
}

/**
 * Simulates LLM token streaming — text appears in irregular bursts of 1-6
 * characters with variable inter-burst pauses, mimicking real inference output.
 *
 * The rhythm is intentionally non-uniform:
 * - Bursts vary in size (1–6 chars, biased toward 2–4)
 * - Inter-burst gaps range from 15ms to 120ms with occasional longer "thinking" pauses
 * - Punctuation and newlines trigger longer pauses (model confidence drops)
 * - Spaces are absorbed into the preceding burst (tokens rarely split mid-word)
 */
export default function TypewriterText({ text, onComplete, enabled = true }: TypewriterTextProps) {
  const [displayedLength, setDisplayedLength] = useState(enabled ? 0 : text.length);
  const onCompleteRef = useRef(onComplete);
  onCompleteRef.current = onComplete;

  useEffect(() => {
    if (!enabled || displayedLength >= text.length) {
      if (displayedLength >= text.length) onCompleteRef.current?.();
      return;
    }

    // Determine burst size: how many characters to reveal at once
    const remaining = text.length - displayedLength;
    let burstSize = pickBurstSize();
    burstSize = Math.min(burstSize, remaining);

    // Extend burst to include trailing spaces (tokens don't split mid-word)
    let end = displayedLength + burstSize;
    while (end < text.length && text[end] === " ") end++;
    burstSize = end - displayedLength;

    // Determine delay before this burst
    const lastChar = displayedLength > 0 ? text[displayedLength - 1] : "";
    const nextChunk = text.slice(displayedLength, displayedLength + burstSize);
    const delay = pickDelay(lastChar, nextChunk);

    const timer = setTimeout(() => {
      setDisplayedLength(displayedLength + burstSize);
    }, delay);

    return () => clearTimeout(timer);
  }, [displayedLength, text, enabled]);

  // Reset when text changes
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally reset only on text change
  useEffect(() => {
    if (enabled) setDisplayedLength(0);
  }, [text]);

  return <>{text.slice(0, displayedLength)}</>;
}

/** Pick a burst size with a distribution biased toward 2-4 chars */
function pickBurstSize(): number {
  const r = Math.random();
  // Distribution: 1 char (10%), 2 chars (25%), 3 chars (30%), 4 chars (20%), 5-6 chars (15%)
  if (r < 0.1) return 1;
  if (r < 0.35) return 2;
  if (r < 0.65) return 3;
  if (r < 0.85) return 4;
  return Math.random() < 0.5 ? 5 : 6;
}

/** Pick an inter-burst delay based on context */
function pickDelay(lastChar: string, nextChunk: string): number {
  // Base delay: random between 15-60ms (fast streaming)
  let base = 15 + Math.random() * 45;

  // Longer pause after sentence-ending punctuation (model "thinking")
  if (lastChar === "." || lastChar === "!" || lastChar === "?") {
    base += 80 + Math.random() * 200;
  }
  // Medium pause after commas, semicolons
  else if (lastChar === "," || lastChar === ";" || lastChar === ":") {
    base += 30 + Math.random() * 80;
  }
  // Pause after newlines
  else if (lastChar === "\n") {
    base += 60 + Math.random() * 150;
  }

  // If next chunk starts a new structural element (dash, number), add hesitation
  if (nextChunk.match(/^[\d\-*]/)) {
    base += 20 + Math.random() * 60;
  }

  // Random "thinking" pauses (~5% of bursts get an extra 100-300ms)
  if (Math.random() < 0.05) {
    base += 100 + Math.random() * 200;
  }

  // Occasional very fast burst (~10% of time, almost instant)
  if (Math.random() < 0.1) {
    base = 5 + Math.random() * 10;
  }

  return base;
}
