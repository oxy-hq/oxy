import { ArrowDown, ArrowUp, Plus } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import CollapsibleSection from "./CollapsibleSection";
import SortRow, { type SortField } from "./SortRow";

interface Order {
  field: string;
  direction: string;
}

interface OrdersDisplayProps {
  orders: Order[];
  editable?: boolean;
  availableFields?: SortField[];
  onAddOrder?: () => void;
  onUpdateOrder?: (index: number, updates: { field: string; direction: "asc" | "desc" }) => void;
  onRemoveOrder?: (index: number) => void;
}

const OrdersDisplay = ({
  orders,
  editable = false,
  availableFields = [],
  onAddOrder,
  onUpdateOrder,
  onRemoveOrder
}: OrdersDisplayProps) => {
  if (!editable && orders.length === 0) return null;

  return (
    <CollapsibleSection title='Ordering' count={orders.length}>
      {editable && onUpdateOrder && onRemoveOrder ? (
        <div className='flex flex-col gap-2'>
          {orders.map((order, index) => (
            <SortRow
              key={`${order.field}-${order.direction}-${index}`}
              order={order as { field: string; direction: "asc" | "desc" }}
              availableFields={availableFields}
              onUpdate={(updates) => onUpdateOrder(index, updates)}
              onRemove={() => onRemoveOrder(index)}
            />
          ))}
          {onAddOrder && (
            <Button variant='ghost' size='sm' className='w-fit' onClick={onAddOrder}>
              <Plus />
              Add ordering
            </Button>
          )}
        </div>
      ) : (
        <div className='flex flex-wrap gap-1.5'>
          {orders.map((order, i) => (
            <span
              key={`order-${order.field}-${order.direction}-${i}`}
              className='inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs'
            >
              <span className='font-medium'>{order.field.split(".").pop()}</span>
              {order.direction.toLowerCase() === "asc" ? (
                <ArrowUp className='h-3 w-3 text-muted-foreground' />
              ) : (
                <ArrowDown className='h-3 w-3 text-muted-foreground' />
              )}
            </span>
          ))}
        </div>
      )}
    </CollapsibleSection>
  );
};

export default OrdersDisplay;
