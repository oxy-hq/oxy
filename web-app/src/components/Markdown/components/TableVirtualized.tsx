import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

type Props = {
  table_id: string;
  tables: string[][][];
};

const TableVirtualized = ({ table_id, tables }: Props) => {
  const id = parseInt(table_id, 10);
  const data = tables[id] || [];

  const parentRef = useRef<HTMLTableElement>(null);

  const header = data[0];
  const rows = data.slice(1);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    estimateSize: () => 34,
    getScrollElement: () => parentRef.current,
    overscan: 20,
  });

  if (data.length === 0) {
    return <div>No data available</div>;
  }

  return (
    <div
      ref={parentRef}
      className="max-h-[370px] py-2 overflow-auto customScrollbar full-width"
    >
      <div style={{ height: `${rowVirtualizer.getTotalSize()}px` }}>
        <table>
          <thead className="text-muted-foreground">
            {header.map((cell, i) => (
              <th
                key={i}
                className="min-w-[140px] px-4 py-2 text-left border-b border-border font-normal"
                title={cell}
              >
                {cell}
              </th>
            ))}
          </thead>

          <tbody>
            {rowVirtualizer.getVirtualItems().map((virtualRow, index) => {
              const row = rows[virtualRow.index];
              return (
                <tr
                  key={virtualRow.index}
                  style={{
                    height: `${virtualRow.size}px`,
                    transform: `translateY(${
                      virtualRow.start - index * virtualRow.size
                    }px)`,
                  }}
                >
                  {row.map((cell, i) => (
                    <td
                      key={i}
                      className="min-w-[140px] px-4 py-2 text-left border-b border-border"
                      title={cell}
                    >
                      {cell}
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default TableVirtualized;
