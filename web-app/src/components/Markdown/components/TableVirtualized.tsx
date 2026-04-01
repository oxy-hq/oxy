import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";

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
    estimateSize: () => 37,
    getScrollElement: () => parentRef.current,
    overscan: 20
  });

  if (data.length === 0) {
    return <div>No data available</div>;
  }

  return (
    <div
      ref={parentRef}
      className='customScrollbar scrollbar-gutter-auto overflow-auto rounded-lg border border-[#27272A]'
      style={{
        position: "relative",
        height: `${Math.min(rowVirtualizer.getTotalSize() + 40, 400)}px`
      }}
    >
      <table className='w-full text-sm'>
        <thead className='text-muted-foreground'>
          {header.map((cell, i) => (
            <th
              key={i}
              className='min-w-[140px] border-[#27272A] border-r border-b px-4 py-2 text-left font-medium last:border-r-0'
              title={cell}
            >
              {cell}
            </th>
          ))}
        </thead>

        <tbody
          style={{
            height: `${rowVirtualizer.getTotalSize()}px`,
            position: "relative"
          }}
        >
          {rowVirtualizer.getVirtualItems().map((virtualRow) => {
            const row = rows[virtualRow.index];
            return (
              <tr
                key={virtualRow.index}
                style={{
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                  position: "absolute",
                  width: "100%",
                  display: "flex"
                }}
              >
                {row.map((cell, i) => (
                  <td
                    key={i}
                    className='w-full min-w-[140px] border-[#27272A] border-r px-4 py-2 text-left last:border-r-0 [tr:not(:last-child)>&]:border-b'
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
  );
};

export default TableVirtualized;
