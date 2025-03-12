"use client";

import { memo } from "react";

import Markdown, { ExtendedComponents } from "react-markdown";
import directive from "remark-directive";
import remarkGfm from "remark-gfm";

import CodeContainer from "./Code";
import { cn } from "@/libs/shadcn/utils";

type Props = {
  content: string;
  className?: string;
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
  code: (props) => <CodeContainer {...props} />,
};

function AnswerContent({ content, className }: Props) {
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <Markdown
        remarkPlugins={[directive, remarkGfm]}
        components={extendedComponents}
      >
        {content}
      </Markdown>
    </div>
  );
}

export default memo(AnswerContent);
