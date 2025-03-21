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
      className="border border-gray-200 !p-4 !rounded-lg"
      language={match[1]}
      style={oneDark}
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
