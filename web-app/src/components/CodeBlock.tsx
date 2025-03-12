import React, { ReactNode } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { css } from "styled-system/css";

type CodeBlockProps = {
  children?: ReactNode;
  className?: string;
};

const CodeBlock: React.FC<CodeBlockProps> = ({ children, className }) => {
  const match = /language-(\w+)/.exec(className || "");
  return match ? (
    <SyntaxHighlighter
      className={css({
        border: "1px solid #E5E7EB",
        borderRadius: "4px",
      })}
      language={match[1]}
      PreTag="div"
      lineProps={{ style: { wordBreak: "break-all", whiteSpace: "pre-wrap" } }}
      wrapLines={true}
    >
      {children}
    </SyntaxHighlighter>
  ) : (
    <code className={className}>{children}</code>
  );
};

export default CodeBlock;
