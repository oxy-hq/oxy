import { cx } from "class-variance-authority";
import type React from "react";
import type { ReactNode } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

type CodeBlockProps = {
  children?: ReactNode;
  className?: string;
};

const CodeBlock: React.FC<CodeBlockProps> = ({ children, className }) => {
  const match = /language-(\w+)/.exec(className || "");
  return match ? (
    <SyntaxHighlighter
      className={cx(
        "border! m-0! max-h-96! rounded-lg! border-[#27272A]! bg-zinc-900! p-4! font-mono text-sm [&>code]:bg-transparent!",
        className
      )}
      language={match ? match[1] : undefined}
      style={oneDark}
      PreTag='div'
      lineProps={{ style: { wordBreak: "break-all", whiteSpace: "pre-wrap" } }}
      wrapLines={true}
    >
      {String(children)}
    </SyntaxHighlighter>
  ) : (
    <code
      className={cx(
        "border! rounded-lg! border-[#27272A]! bg-zinc-900! px-1.5 py-0.5 font-mono text-xs",
        className
      )}
    >
      {children}
    </code>
  );
};

export default CodeBlock;
