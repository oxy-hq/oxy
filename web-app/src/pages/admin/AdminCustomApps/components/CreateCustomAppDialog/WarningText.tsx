/**
 * Render a warning string that may contain inline `https?://...` URLs.
 * URLs are converted to underlined anchors that open in a new tab.
 * The rest of the text renders as-is. Used by both `BundleProbeSummary`
 * and `LinkPlanSummary` so all probe / link warnings consistently
 * surface migration guides + reference links.
 */

const URL_RE = /(https?:\/\/[^\s]+)/g;

export const WarningText = ({ text }: { text: string }) => {
  const parts = text.split(URL_RE);
  return (
    <>
      {parts.map((part, i) =>
        part.match(URL_RE) ? (
          <a
            // biome-ignore lint/suspicious/noArrayIndexKey: parts are pure text segments; index is stable for a given input
            key={i}
            href={part}
            target='_blank'
            rel='noopener noreferrer'
            className='underline underline-offset-2'
          >
            {part}
          </a>
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
};
