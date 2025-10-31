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
  onArtifactClick?: (id: string) => void;
}

const sanitizeSchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    chart: ["chart_src"],
    artifact: ["artifactId", "kind", "title", "is_verified"],
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
  onArtifactClick?: (id: string) => void;
};

const getExtendedComponents = (data?: MarkdownData): ExtendedComponents => ({
  h1: ({ children }) => (
    <h1 className="text-3xl font-bold mt-6 mb-4">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-2xl font-semibold mt-5 mb-3">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="text-xl font-medium mt-4 mb-2">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="text-lg font-medium mt-4 mb-2">{children}</h4>
  ),
  h5: ({ children }) => (
    <h5 className="text-md font-medium mt-4 mb-2">{children}</h5>
  ),
  p: ({ children }) => <p className="text-sm leading-7 mb-2">{children}</p>,
  ul: ({ children }) => (
    <ul className="list-disc pl-6 mb-2 [&>li]:mb-1">{children}</ul>
  ),
  ol: ({ children, start, ...props }) => (
    <ol className="list-decimal pl-6 mb-2 [&>li]:mb-1" start={start} {...props}>
      {children}
    </ol>
  ),
  li: ({ children }) => <li>{children}</li>,
  pre: ({ children }) => (
    <pre className="overflow-auto text-sm">{children}</pre>
  ),
  blockquote: ({ children }) => (
    <blockquote className="border-l-4 pl-4 italic text-muted-foreground my-4">
      {children}
    </blockquote>
  ),
  table: ({ children, ...props }) => (
    <div className="max-h-96 overflow-auto customScrollbar scrollbar-gutter-auto rounded-lg border border-[#27272A]">
      <table className="w-full border-collapse text-sm" {...props}>
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
      className="min-w-[140px] px-4 py-2 text-left border-b border-r border-[#27272A] font-medium last:border-r-0"
      {...props}
    >
      {children}
    </th>
  ),
  td: ({ children, ...props }) => (
    <td
      className="min-w-[140px] px-4 py-2 text-left border-r border-[#27272A] last:border-r-0 [tr:not(:last-child)>&]:border-b"
      {...props}
    >
      {children}
    </td>
  ),
  code: (props) => <CodeBlock {...props} />,
  chart: (props) => <ChartContainer {...props} />,
  artifact: (props) => (
    <ArtifactContainer {...props} onClick={data?.onArtifactClick} />
  ),
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

function Markdown({ children, onArtifactClick }: Props) {
  const { newMarkdown, tables } = extractLargeTables(children || "");
  return (
    <div
      style={{
        fontSize: "14px",
      }}
    >
      <ReactMarkdown
        remarkPlugins={[
          directive,
          remarkGfm,
          ChartPlugin,
          ArtifactPlugin,
          TableVirtualizedPlugin,
        ]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, sanitizeSchema]]}
        components={getExtendedComponents({ tables, onArtifactClick })}
      >
        {newMarkdown}
      </ReactMarkdown>
    </div>
  );
}

export default memo(Markdown);
