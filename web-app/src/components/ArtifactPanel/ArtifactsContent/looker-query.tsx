import FieldList from "@/components/SemanticQueryPanel/FieldList";
import FiltersDisplay from "@/components/SemanticQueryPanel/FiltersDisplay";
import LimitOffset from "@/components/SemanticQueryPanel/LimitOffset";
import OrdersDisplay from "@/components/SemanticQueryPanel/OrdersDisplay";
import SqlDisplay from "@/components/SemanticQueryPanel/SqlDisplay";
import TopicHeader from "@/components/SemanticQueryPanel/TopicHeader";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import type { LookerQueryArtifact } from "@/types/artifact";

type Props = {
  artifact: LookerQueryArtifact;
};

type LookerFilter = {
  field: string;
  op: string;
  value: string;
};

type LookerOrder = {
  field: string;
  direction: string;
};

const toLookerFilters = (filters: Record<string, string> | undefined): LookerFilter[] => {
  if (!filters) return [];

  return Object.entries(filters).map(([field, value]) => ({
    field,
    op: "=",
    value
  }));
};

const toLookerOrders = (sorts: string[] | undefined): LookerOrder[] => {
  if (!sorts) return [];

  return sorts
    .map((sort) => sort.trim())
    .filter(Boolean)
    .map((sort) => {
      const isDesc = sort.startsWith("-");
      return {
        field: isDesc ? sort.slice(1) : sort,
        direction: isDesc ? "desc" : "asc"
      };
    });
};

const LookerQueryArtifactPanel = ({ artifact }: Props) => {
  const { model, explore, fields, filters, sorts, limit, sql, result, result_file } =
    artifact.content.value;

  return (
    <div className='flex h-full flex-col overflow-y-auto'>
      <TopicHeader topic={`${model}.${explore}`} />

      <FieldList title='Fields' fields={fields} />
      <FiltersDisplay filters={toLookerFilters(filters)} />
      <OrdersDisplay orders={toLookerOrders(sorts)} />
      <LimitOffset limit={limit} />

      <SqlDisplay sql={sql} defaultOpen={true} />

      {(result || result_file) && (
        <div className='min-h-50 flex-1'>
          <SqlResultsTable result={result} resultFile={result_file} />
        </div>
      )}
    </div>
  );
};

export default LookerQueryArtifactPanel;
