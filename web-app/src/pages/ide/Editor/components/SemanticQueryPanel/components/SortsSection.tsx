import { SemanticQueryOrder } from "@/services/api/semantic";
import SortRow from "./SortRow";

interface SortsSectionProps {
  orders: SemanticQueryOrder[];
  availableFields: { label: string; value: string }[];
  onUpdateOrder: (index: number, updates: SemanticQueryOrder) => void;
  onRemoveOrder: (index: number) => void;
}

const SortsSection = ({
  orders,
  availableFields,
  onUpdateOrder,
  onRemoveOrder,
}: SortsSectionProps) => {
  if (orders.length === 0) return null;

  return (
    <div className="border-b p-3 space-y-2 w-full flex flex-col">
      <div>Sorts</div>
      {orders.map((order, index) => (
        <SortRow
          key={`${index}`}
          order={order}
          availableFields={availableFields}
          onUpdate={(updates) => onUpdateOrder(index, updates)}
          onRemove={() => onRemoveOrder(index)}
        />
      ))}
    </div>
  );
};

export default SortsSection;
