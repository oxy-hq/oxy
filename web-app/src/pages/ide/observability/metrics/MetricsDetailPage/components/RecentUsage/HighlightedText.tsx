interface HighlightedTextProps {
  text: string;
  highlight: string;
}

export default function HighlightedText({ text, highlight }: HighlightedTextProps) {
  if (!text || !highlight) return <span>{text}</span>;

  const regex = new RegExp(`(${highlight.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  const parts = text.split(regex);

  return (
    <span>
      {parts.map((part, i) =>
        regex.test(part) ? (
          <mark key={i} className='rounded bg-yellow-500/30 px-0.5 font-medium text-yellow-200'>
            {part}
          </mark>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </span>
  );
}
