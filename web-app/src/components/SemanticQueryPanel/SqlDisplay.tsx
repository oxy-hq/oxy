import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import CollapsibleSection from "./CollapsibleSection";

interface SqlDisplayProps {
  sql: string;
  defaultOpen?: boolean;
}

const SqlDisplay = ({ sql, defaultOpen = false }: SqlDisplayProps) => {
  if (!sql) return null;

  return (
    <CollapsibleSection title='Generated SQL' defaultOpen={defaultOpen}>
      <div className='[&_pre]:!my-0 customScrollbar text-xs [&_pre]:max-h-[300px] [&_pre]:overflow-auto'>
        <SyntaxHighlighter
          language='sql'
          style={oneDark}
          wrapLines={true}
          customStyle={{ margin: 0, borderRadius: "0.5rem" }}
          lineProps={{
            style: { wordBreak: "break-all", whiteSpace: "pre-wrap" }
          }}
          className='font-mono text-xs'
        >
          {sql}
        </SyntaxHighlighter>
      </div>
    </CollapsibleSection>
  );
};

export default SqlDisplay;
