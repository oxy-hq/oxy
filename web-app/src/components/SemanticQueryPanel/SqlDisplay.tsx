import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import usePrismTheme from "@/hooks/usePrismTheme";
import CollapsibleSection from "./CollapsibleSection";

interface SqlDisplayProps {
  sql: string;
  defaultOpen?: boolean;
}

const SqlDisplay = ({ sql, defaultOpen = false }: SqlDisplayProps) => {
  const prismTheme = usePrismTheme();
  if (!sql) return null;

  return (
    <CollapsibleSection title='Generated SQL' defaultOpen={defaultOpen}>
      <div className='[&_pre]:!my-0 text-xs [&_pre]:max-h-[300px] [&_pre]:overflow-auto'>
        <SyntaxHighlighter
          language='sql'
          style={prismTheme}
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
