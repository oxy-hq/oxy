import { cx } from "class-variance-authority";
import React, { ReactNode } from "react";
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
        "p-4! m-0! max-h-96! rounded-lg! border! border-[#27272A]! bg-zinc-900! [&>code]:bg-transparent! text-sm font-mono",
        className,
      )}
      language={match ? match[1] : undefined}
      style={oneDark}
      PreTag="div"
      lineProps={{ style: { wordBreak: "break-all", whiteSpace: "pre-wrap" } }}
      wrapLines={true}
    >
      {String(children)}
    </SyntaxHighlighter>
  ) : (
    <code
      className={cx(
        "bg-zinc-900! px-1.5 py-0.5 rounded-lg! border! border-[#27272A]! text-xs font-mono",
        className,
      )}
    >
      {children}
    </code>
  );
};

export default CodeBlock;
