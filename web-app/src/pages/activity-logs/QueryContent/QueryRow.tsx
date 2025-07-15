import CodeBlock from "@/components/Markdown/components/CodeBlock";

const QueryRow = ({ queryItem }: { queryItem: Record<string, unknown> }) => {
  const query = queryItem.query as string | undefined;

  return (
    <div>
      {query && (
        <div className="overflow-x-auto">
          <CodeBlock className="language-sql !text-sm !border-[transparent] !p-2">
            {query}
          </CodeBlock>
        </div>
      )}
    </div>
  );
};

export default QueryRow;
