import { ArrowDown, ArrowUp } from "lucide-react";
import CollapsibleSection from "./CollapsibleSection";

interface Order {
  field: string;
  direction: string;
}

interface OrdersDisplayProps {
  orders: Order[];
}

const OrdersDisplay = ({ orders }: OrdersDisplayProps) => {
  if (orders.length === 0) return null;

  return (
    <CollapsibleSection title='Ordering' count={orders.length}>
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
    </CollapsibleSection>
  );
};

export default OrdersDisplay;
