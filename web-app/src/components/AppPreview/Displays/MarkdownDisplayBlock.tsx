import nunjucks from "nunjucks";
import Markdown from "@/components/Markdown";
import type { DataContainer, MarkdownDisplay, TableData } from "@/types/app";

const nunjucksEnv = new nunjucks.Environment(null, { autoescape: false });

function flattenTablesForTemplate(data: DataContainer): DataContainer {
  if (data && typeof data === "object" && !Array.isArray(data) && "file_path" in data) {
    const json = (data as TableData).json;
    return json ? `\`\`\`json\n${json}\n\`\`\`` : "";
  }
  if (Array.isArray(data)) {
    return data.map(flattenTablesForTemplate);
  }
  if (data && typeof data === "object") {
    const result: Record<string, DataContainer> = {};
    for (const [key, value] of Object.entries(data)) {
      result[key] = flattenTablesForTemplate(value as DataContainer);
    }
    return result;
  }
  return data;
}

export const MarkdownDisplayBlock = ({
  display,
  data
}: {
  display: MarkdownDisplay;
  data?: DataContainer;
}) => {
  const dataContainer = flattenTablesForTemplate(data || {});
  const rendered_content = nunjucksEnv.renderString(display.content, dataContainer as object);
  return (
    <div className='markdown-display' data-testid='app-markdown-display-block'>
      <Markdown>{rendered_content}</Markdown>
    </div>
  );
};
