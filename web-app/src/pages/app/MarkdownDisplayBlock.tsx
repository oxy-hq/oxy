import nunjucks from "nunjucks";
import { DataContainer, MarkdownDisplay } from "@/types/app";
import Markdown from "@/components/Markdown";

export const MarkdownDisplayBlock = ({
  display,
  data,
}: {
  display: MarkdownDisplay;
  data: DataContainer;
}) => {
  const rendered_content = nunjucks.renderString(
    display.content,
    data as object,
  );
  return (
    <div className="markdown-display">
      <Markdown>{rendered_content}</Markdown>
    </div>
  );
};
