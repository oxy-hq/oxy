"use client";

import { memo } from "react";

import ReactMarkdown, { ExtendedComponents } from "react-markdown";
import directive from "remark-directive";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import ArtifactPlugin from "./plugins/ArtifactPlugin";
import ChartPlugin from "./plugins/ChartPlugin";
import ChartContainer from "./components/Chart";
import ArtifactContainer from "./components/Artifact";
import TableVirtualized from "./components/TableVirtualized";
import CodeBlock from "./components/CodeBlock";
import { extractLargeTables } from "./utils/extractLargeTables";
import TableVirtualizedPlugin from "./plugins/TableVirtualizedPlugin";

interface MarkdownData {
  tables?: string[][][];
}

const sanitizeSchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    chart: ["chart_src"],
    artifact: ["kind", "title", "is_verified"],
    table_virtualized: ["table_id"],
  },
  tagNames: [
    ...(defaultSchema.tagNames || []),
    "chart",
    "artifact",
    "table_virtualized",
  ],
};

type Props = {
  children: string;
};

const getExtendedComponents = (data?: MarkdownData): ExtendedComponents => ({
  table: ({ children, ...props }) => (
    <div className="overflow-auto customScrollbar">
      <table className="w-full border-collapse border-hidden" {...props}>
        {children}
      </table>
    </div>
  ),
  thead: ({ children, ...props }) => (
    <thead className="text-muted-foreground" {...props}>
      {children}
    </thead>
  ),
  th: ({ children, ...props }) => (
    <th
      className="min-w-[140px] px-4 py-2 text-left border-b border-border font-normal"
      {...props}
    >
      {children}
    </th>
  ),
  td: ({ children, ...props }) => (
    <td
      className="min-w-[140px] px-4 py-2 text-left border-b border-border"
      {...props}
    >
      {children}
    </td>
  ),
  code: (props) => <CodeBlock {...props} />,
  chart: (props) => <ChartContainer {...props} />,
  artifact: (props) => <ArtifactContainer {...props} />,
  table_virtualized: (props) => (
    <TableVirtualized {...props} tables={data?.tables ?? []} />
  ),
  a: ({ children, ...props }) => (
    <a
      className="text-primary hover:underline"
      {...props}
      target="_blank"
      rel="noopener noreferrer"
    >
      {children}
    </a>
  ),
});

function Markdown({ children }: Props) {
  const { newMarkdown, tables } = extractLargeTables(children);
  return (
    <ReactMarkdown
      remarkPlugins={[
        directive,
        remarkGfm,
        ChartPlugin,
        ArtifactPlugin,
        TableVirtualizedPlugin,
      ]}
      rehypePlugins={[rehypeRaw, [rehypeSanitize, sanitizeSchema]]}
      components={getExtendedComponents({ tables })}
    >
      {newMarkdown}
    </ReactMarkdown>
  );
}

export default memo(Markdown);
