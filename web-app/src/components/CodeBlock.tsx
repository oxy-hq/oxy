import React, { ReactNode } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";

type CodeBlockProps = {
  children?: ReactNode;
  className?: string;
};

const CodeBlock: React.FC<CodeBlockProps> = ({ children, className }) => {
  const match = /language-(\w+)/.exec(className || "");
  return match ? (
    <SyntaxHighlighter
      className="border border-gray-200 rounded"
      language={match[1]}
      PreTag="div"
      lineProps={{ style: { wordBreak: "break-all", whiteSpace: "pre-wrap" } }}
      wrapLines={true}
    >
      {String(children)}
    </SyntaxHighlighter>
  ) : (
    <code className={className}>{children}</code>
  );
};

export default CodeBlock;
