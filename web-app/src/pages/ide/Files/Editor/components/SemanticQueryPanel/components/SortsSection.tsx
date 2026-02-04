import type { Field, Order } from "../../../types";
import SortRow from "./SortRow";

interface SortsSectionProps {
  orders: Order[];
  availableFields: Field[];
  onUpdateOrder: (index: number, updates: Order) => void;
  onRemoveOrder: (index: number) => void;
}

const SortsSection = ({
  orders,
  availableFields,
  onUpdateOrder,
  onRemoveOrder
}: SortsSectionProps) => {
  if (orders.length === 0) return null;

  return (
    <div className='flex w-full flex-col space-y-2 border-b p-3'>
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
