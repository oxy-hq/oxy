import nunjucks from "nunjucks";
import Markdown from "@/components/Markdown";
import type { DataContainer, MarkdownDisplay } from "@/types/app";

export const MarkdownDisplayBlock = ({
  display,
  data
}: {
  display: MarkdownDisplay;
  data?: DataContainer;
}) => {
  const dataContainer = data || {};
  const rendered_content = nunjucks.renderString(display.content, dataContainer as object);
  return (
    <div className='markdown-display' data-testid='app-markdown-display-block'>
      <Markdown>{rendered_content}</Markdown>
    </div>
  );
};
