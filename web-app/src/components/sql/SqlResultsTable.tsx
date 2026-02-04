import { memo, useEffect, useMemo, useState } from "react";
import EmptyState from "@/components/ui/EmptyState";
import { VirtualizedTable } from "@/components/ui/VirtualizedTable";

interface ResultsProps {
  result?: string[][];
  resultFile?: string;
  hideDownload?: boolean;
}

interface DataCellProps {
  cell: string;
  rowIdx: number;
  cellIdx: number;
  isSelected: boolean;
  onClick: () => void;
}

const DataCell = memo(
  ({ cell, isSelected, onClick }: DataCellProps) => (
    <div
      className={`flex h-7 cursor-pointer items-center overflow-hidden border-r px-3 py-1 last:border-r-0 ${
        isSelected ? "bg-primary/20 ring-2 ring-primary ring-inset" : "hover:bg-muted/50"
      }`}
      title={cell}
      onClick={onClick}
    >
      <span className='truncate'>{cell}</span>
    </div>
  ),
  (prevProps, nextProps) =>
    prevProps.cell === nextProps.cell && prevProps.isSelected === nextProps.isSelected
);

DataCell.displayName = "DataCell";

const ArrayBasedTable = ({ result }: { result: string[][] }) => {
  const [selectedCell, setSelectedCell] = useState<{
    row: number;
    col: number;
  } | null>(null);

  // Track custom column widths (null means use default)
  const [customColumnWidths, setCustomColumnWidths] = useState<Map<number, number>>(new Map());

  const [resizingColumn, setResizingColumn] = useState<{
    index: number;
    startX: number;
    startWidth: number;
  } | null>(null);

  // Compute column widths: use custom width if available, otherwise default
  const columnWidths = useMemo(() => {
    if (result.length === 0) return [];
    const numCols = result[0].length;
    return Array.from({ length: numCols }, (_, i) => customColumnWidths.get(i) ?? 150);
  }, [result, customColumnWidths]);

  // All hooks must be called before any returns
  useEffect(() => {
    if (result.length === 0) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "c" && selectedCell) {
        const value = result[selectedCell.row][selectedCell.col];
        navigator.clipboard.writeText(value).catch((err) => {
          console.error("Failed to copy:", err);
        });
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedCell, result]);

  // Handle column resizing
  useEffect(() => {
    if (!resizingColumn) return;

    let rafId: number | null = null;
    let latestWidth: number | null = null;

    const updateColumnWidth = () => {
      if (latestWidth !== null) {
        setCustomColumnWidths((prev) => {
          const updated = new Map(prev);
          updated.set(resizingColumn.index, latestWidth!);
          return updated;
        });
      }
      rafId = null;
    };

    const handleMouseMove = (e: MouseEvent) => {
      const deltaX = e.clientX - resizingColumn.startX;
      const newWidth = Math.max(50, resizingColumn.startWidth + deltaX);
      latestWidth = newWidth;

      // Throttle updates using requestAnimationFrame
      if (rafId === null) {
        rafId = requestAnimationFrame(updateColumnWidth);
      }
    };

    const handleMouseUp = () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      setResizingColumn(null);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);

    return () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [resizingColumn]);

  if (result.length === 0) {
    return (
      <EmptyState
        className='h-full'
        title='No results to display'
        description='Run the query to see the results'
      />
    );
  }

  const numColumns = result[0].length;
  const columnWidthsString = columnWidths.map((w: number) => `${w}px`).join(" ");
  const gridTemplateColumns =
    columnWidths.length > 0
      ? `60px ${columnWidthsString}`
      : `60px repeat(${numColumns}, minmax(150px, 1fr))`;

  const handleResizeStart = (colIdx: number, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setResizingColumn({
      index: colIdx,
      startX: e.clientX,
      startWidth: columnWidths[colIdx] || 150
    });
  };

  return (
    <div className='customScrollbar flex h-full min-h-0 flex-col overflow-auto font-mono text-xs'>
      <div className='flex min-w-fit flex-col'>
        {/* Fixed Header */}
        <div
          className='sticky top-0 z-10 grid flex-shrink-0 border-b bg-muted'
          style={{ gridTemplateColumns }}
        >
          {/* Row number header */}
          <div className='flex h-8 items-center justify-center border-r bg-muted/80 px-3 font-semibold uppercase' />

          {result[0].map((cell, idx) => (
            <div
              key={idx}
              className={`relative flex h-8 cursor-pointer items-center overflow-hidden border-r px-3 font-semibold uppercase last:border-r-0 ${
                selectedCell?.row === 0 && selectedCell?.col === idx
                  ? "bg-primary/20 ring-2 ring-primary ring-inset"
                  : "hover:bg-muted-foreground/10"
              }`}
              onClick={() => setSelectedCell({ row: 0, col: idx })}
              title={cell}
            >
              <span className='truncate'>{cell}</span>
              <div
                className='absolute top-0 right-0 bottom-0 w-1 cursor-col-resize hover:bg-primary/50 active:bg-primary'
                onMouseDown={(e) => handleResizeStart(idx, e)}
              />
            </div>
          ))}
        </div>

        {/* Scrollable Body */}
        <div className='flex flex-col'>
          {result.slice(1).map((row, rowIdx) => (
            <div key={rowIdx} className='grid border-b' style={{ gridTemplateColumns }}>
              {/* Row number */}
              <div className='flex h-7 items-center justify-center border-r bg-muted/30 px-3 py-1 text-muted-foreground'>
                {rowIdx + 1}
              </div>

              {row.map((cell, cellIdx) => (
                <DataCell
                  key={cellIdx}
                  cell={cell}
                  rowIdx={rowIdx}
                  cellIdx={cellIdx}
                  isSelected={selectedCell?.row === rowIdx + 1 && selectedCell?.col === cellIdx}
                  onClick={() => setSelectedCell({ row: rowIdx + 1, col: cellIdx })}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

const SqlResultsTable = ({ result, resultFile }: ResultsProps) => {
  // If we have a result file, use the VirtualizedTable component
  if (resultFile) {
    return <VirtualizedTable key={resultFile} filePath={resultFile} />;
  }

  // If we have array results, use the array-based table
  if (result && result.length > 0) {
    return <ArrayBasedTable result={result} />;
  }

  // No results to display
  return (
    <EmptyState
      className='h-full'
      title='No results to display'
      description='Run the query to see the results'
    />
  );
};

export default SqlResultsTable;
