"use client";

import { memo } from "react";

import ReactMarkdown, { type ExtendedComponents } from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import directive from "remark-directive";
import remarkGfm from "remark-gfm";
import ArtifactContainer from "./components/Artifact";
import ChartContainer from "./components/Chart";
import CodeBlock from "./components/CodeBlock";
import ReasoningContainer from "./components/Reasoning";
import TableVirtualized from "./components/TableVirtualized";
import ArtifactPlugin from "./plugins/ArtifactPlugin";
import ChartPlugin from "./plugins/ChartPlugin";
import ReasoningPlugin from "./plugins/ReasoningPlugin";
import TableVirtualizedPlugin from "./plugins/TableVirtualizedPlugin";
import { extractLargeTables } from "./utils/extractLargeTables";

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
    reasoning: []
  },
  tagNames: [
    ...(defaultSchema.tagNames || []),
    "chart",
    "artifact",
    "table_virtualized",
    "reasoning"
  ]
};

type Props = {
  children: string;
  onArtifactClick?: (id: string) => void;
};

const getExtendedComponents = (data?: MarkdownData): ExtendedComponents => ({
  h1: ({ children }) => <h1 className='mt-6 mb-4 font-bold text-3xl'>{children}</h1>,
  h2: ({ children }) => <h2 className='mt-5 mb-3 font-semibold text-2xl'>{children}</h2>,
  h3: ({ children }) => <h3 className='mt-4 mb-2 font-medium text-xl'>{children}</h3>,
  h4: ({ children }) => <h4 className='mt-4 mb-2 font-medium text-lg'>{children}</h4>,
  h5: ({ children }) => <h5 className='mt-4 mb-2 font-medium text-md'>{children}</h5>,
  p: ({ children }) => <p className='mb-2 text-sm leading-7'>{children}</p>,
  ul: ({ children }) => <ul className='mb-2 list-disc pl-6 [&>li]:mb-1'>{children}</ul>,
  ol: ({ children, start, ...props }) => (
    <ol className='mb-2 list-decimal pl-6 [&>li]:mb-1' start={start} {...props}>
      {children}
    </ol>
  ),
  li: ({ children }) => <li>{children}</li>,
  pre: ({ children }) => <pre className='overflow-auto text-sm'>{children}</pre>,
  blockquote: ({ children }) => (
    <blockquote className='my-4 border-l-4 pl-4 text-muted-foreground italic'>
      {children}
    </blockquote>
  ),
  table: ({ children, ...props }) => (
    <div className='customScrollbar scrollbar-gutter-auto max-h-96 overflow-auto rounded-lg border border-[#27272A]'>
      <table className='w-full border-collapse text-sm' {...props}>
        {children}
      </table>
    </div>
  ),
  thead: ({ children, ...props }) => (
    <thead className='text-muted-foreground' {...props}>
      {children}
    </thead>
  ),
  th: ({ children, ...props }) => (
    <th
      className='min-w-[140px] border-[#27272A] border-r border-b px-4 py-2 text-left font-medium last:border-r-0'
      {...props}
    >
      {children}
    </th>
  ),
  td: ({ children, ...props }) => (
    <td
      className='min-w-[140px] border-[#27272A] border-r px-4 py-2 text-left last:border-r-0 [tr:not(:last-child)>&]:border-b'
      {...props}
    >
      {children}
    </td>
  ),
  code: (props) => <CodeBlock {...props} />,
  chart: (props) => <ChartContainer {...props} />,
  artifact: (props) => <ArtifactContainer {...props} onClick={data?.onArtifactClick} />,
  reasoning: (props: { children?: React.ReactNode }) => (
    <ReasoningContainer>{props.children}</ReasoningContainer>
  ),
  table_virtualized: (props) => <TableVirtualized {...props} tables={data?.tables ?? []} />,
  a: ({ children, ...props }) => (
    <a
      className='text-primary hover:underline'
      {...props}
      target='_blank'
      rel='noopener noreferrer'
    >
      {children}
    </a>
  )
});

function Markdown({ children, onArtifactClick }: Props) {
  const { newMarkdown, tables } = extractLargeTables(children || "");

  return (
    <div
      style={{
        fontSize: "14px"
      }}
      data-testid='agent-response-text'
    >
      <ReactMarkdown
        remarkPlugins={[
          directive,
          remarkGfm,
          ChartPlugin,
          ArtifactPlugin,
          ReasoningPlugin,
          TableVirtualizedPlugin
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
