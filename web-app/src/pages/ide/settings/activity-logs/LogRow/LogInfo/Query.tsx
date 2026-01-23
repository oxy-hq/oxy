import CodeBlock from "@/components/Markdown/components/CodeBlock";
import { QueryItem } from "@/types/logs";
import Metadata from "./Metadata";

const Query = ({ queryItem }: { queryItem: QueryItem }) => {
  const query = queryItem.query as string | undefined;

  return (
    <div className="overflow-x-auto flex flex-col border rounded-md p-2">
      <CodeBlock className="language-sql !text-sm !border-[transparent] !p-2 !m-0 !mb-2">
        {query}
      </CodeBlock>
      <Metadata queryItem={queryItem} />
    </div>
  );
};

export default Query;
