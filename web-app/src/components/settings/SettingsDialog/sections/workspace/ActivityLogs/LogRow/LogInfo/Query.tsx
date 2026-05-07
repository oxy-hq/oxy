import CodeBlock from "@/components/Markdown/components/CodeBlock";
import type { QueryItem } from "@/types/logs";
import Metadata from "./Metadata";

const Query = ({ queryItem }: { queryItem: QueryItem }) => {
  const query = queryItem.query as string | undefined;

  return (
    <div className='flex flex-col overflow-x-auto rounded-md border p-2'>
      <CodeBlock className='language-sql !m-0 !mb-2 !border-[transparent] !p-2 !text-sm'>
        {query}
      </CodeBlock>
      <Metadata queryItem={queryItem} />
    </div>
  );
};

export default Query;
