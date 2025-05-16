"use client";

import { memo } from "react";

import ReactMarkdown, { ExtendedComponents } from "react-markdown";
import directive from "remark-directive";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";

const sanitizeSchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    chart: ["chart_src"],
  },
  tagNames: [...(defaultSchema.tagNames || []), "chart"],
};

type Props = {
  children: string;
  plugins?: unknown[];
  components?: ExtendedComponents;
};

const extendedComponents: ExtendedComponents = {
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
};

function Markdown({ children, plugins, components }: Props) {
  return (
    <p className="markdown">
      <ReactMarkdown
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        remarkPlugins={[directive, remarkGfm, ...((plugins as any[]) || [])]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, sanitizeSchema]]}
        components={{ ...extendedComponents, ...components }}
      >
        {children}
      </ReactMarkdown>
    </p>
  );
}

export default memo(Markdown);
