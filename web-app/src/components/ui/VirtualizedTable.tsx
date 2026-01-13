import { useEffect, useState, useCallback, memo, useMemo, useRef } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { ChevronDown, ChevronUp, ChevronsUpDown, Download } from "lucide-react";
import { getDuckDB } from "@/libs/duckdb";
import { registerAuthenticatedParquetFile } from "@/libs/duckdb";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import Papa from "papaparse";
import {
  getArrowValueWithType,
  getArrowFieldType,
} from "@/components/AppPreview/Displays/utils";

interface VirtualizedTableProps {
  filePath: string;
  pageSize?: number;
  maxHeight?: string;
}

type SortDirection = "asc" | "desc" | null;

interface SortConfig {
  column: string | null;
  direction: SortDirection;
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
      className={`h-7 px-3 py-1 flex items-center border-r last:border-r-0 overflow-hidden cursor-pointer ${
        isSelected
          ? "bg-primary/20 ring-2 ring-primary ring-inset"
          : "hover:bg-muted/50"
      }`}
      title={cell}
      onClick={onClick}
    >
      <span className="truncate">{cell}</span>
    </div>
  ),
  (prevProps, nextProps) =>
    prevProps.cell === nextProps.cell &&
    prevProps.isSelected === nextProps.isSelected,
);

DataCell.displayName = "DataCell";

export const VirtualizedTable = ({
  filePath,
  pageSize = 1000,
  maxHeight = undefined,
}: VirtualizedTableProps) => {
  const { project, branchName } = useCurrentProjectBranch();
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [columns, setColumns] = useState<string[]>([]);
  const [data, setData] = useState<unknown[][]>([]);
  const [totalRows, setTotalRows] = useState(0);
  const [currentPage, setCurrentPage] = useState(0);
  const [sortConfig, setSortConfig] = useState<SortConfig>({
    column: null,
    direction: null,
  });
  const [tableName, setTableName] = useState<string>("");

  // Use refs for columns and schema to avoid triggering refetches
  const columnsRef = useRef<string[]>([]);
  const schemaRef = useRef<unknown>(null);
  // Track which filePath is currently registered to detect changes
  const registeredFilePathRef = useRef<string>("");
  const [selectedCell, setSelectedCell] = useState<{
    row: number;
    col: number;
  } | null>(null);

  // Track custom column widths (null means use default)
  const [customColumnWidths, setCustomColumnWidths] = useState<
    Map<number, number>
  >(new Map());

  const [resizingColumn, setResizingColumn] = useState<{
    index: number;
    startX: number;
    startWidth: number;
  } | null>(null);

  // Compute column widths: use custom width if available, otherwise default
  const columnWidths = useMemo(() => {
    if (columns.length === 0) return [];
    const numCols = columns.length;
    return Array.from(
      { length: numCols },
      (_, i) => customColumnWidths.get(i) ?? 150,
    );
  }, [columns, customColumnWidths]);

  const loadData = useCallback(
    async (page: number, sort: SortConfig) => {
      try {
        const db = await getDuckDB();
        const conn = await db.connect();

        // Use a local variable to track the table name for this execution
        let tableToQuery = tableName;

        // Register the file if not already registered OR if filePath has changed
        const needsRegistration =
          !tableName || registeredFilePathRef.current !== filePath;

        if (needsRegistration) {
          console.log("Registering Parquet file:", filePath);
          const registeredName = await registerAuthenticatedParquetFile(
            filePath,
            project.id,
            branchName,
          );
          console.log("Registered table name:", registeredName);
          setTableName(registeredName);
          registeredFilePathRef.current = filePath; // Track the registered filePath
          tableToQuery = registeredName; // Use the newly registered name for this query

          // Get schema and total count
          const countResult = await conn.query(
            `SELECT COUNT(*) as count FROM "${registeredName}"`,
          );
          setTotalRows(Number(countResult.toArray()[0].count));

          // Get columns
          const schemaResult = await conn.query(
            `SELECT * FROM "${registeredName}" LIMIT 0`,
          );
          const cols = schemaResult.schema.fields.map((f) => f.name);
          columnsRef.current = cols;
          schemaRef.current = schemaResult.schema;
          setColumns(cols);
        }

        // Build query with sorting
        const offset = page * pageSize;
        let query = `SELECT * FROM "${tableToQuery}"`;

        // Add sorting
        if (sort.column && sort.direction) {
          query += ` ORDER BY "${sort.column}" ${sort.direction.toUpperCase()}`;
        }

        // Add pagination
        query += ` LIMIT ${pageSize} OFFSET ${offset}`;

        const result = await conn.query(query);
        const rows = result.toArray();

        // Convert to array format for rendering
        const formattedData = rows.map((row) =>
          columnsRef.current.map((col) => {
            const value = (row as Record<string, unknown>)[col];
            if (schemaRef.current) {
              const fieldType = getArrowFieldType(col, result.schema);
              return fieldType
                ? getArrowValueWithType(value, fieldType)
                : value;
            }
            return value;
          }),
        );

        setData(formattedData);
      } catch (err) {
        console.error("Error loading data:", err);
        throw err;
      }
    },
    [filePath, project.id, branchName, pageSize, tableName],
  );

  useEffect(() => {
    let cancelled = false;

    const fetchData = async () => {
      setIsLoading(true);
      setError(null);

      try {
        await loadData(currentPage, sortConfig);
        if (!cancelled) {
          setIsLoading(false);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Failed to load data");
          setIsLoading(false);
        }
      }
    };

    fetchData();

    return () => {
      cancelled = true;
    };
  }, [currentPage, sortConfig, loadData]);

  // Handle keyboard shortcuts for copy
  useEffect(() => {
    if (data.length === 0) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "c" && selectedCell) {
        const value = data[selectedCell.row - 1]?.[selectedCell.col];
        if (value !== undefined) {
          navigator.clipboard.writeText(String(value ?? "")).catch((err) => {
            console.error("Failed to copy:", err);
          });
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedCell, data]);

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

  const handleSort = (column: string) => {
    setSortConfig((prev) => {
      if (prev.column === column) {
        // Cycle through: asc -> desc -> null
        if (prev.direction === "asc") {
          return { column, direction: "desc" };
        } else if (prev.direction === "desc") {
          return { column: null, direction: null };
        }
      }
      return { column, direction: "asc" };
    });
    setCurrentPage(0); // Reset to first page when sorting
  };

  const handleDownloadCsv = async () => {
    try {
      const db = await getDuckDB();
      const conn = await db.connect();

      // Get all data (up to a reasonable limit)
      let query = `SELECT * FROM "${tableName}"`;
      if (sortConfig.column && sortConfig.direction) {
        query += ` ORDER BY "${sortConfig.column}" ${sortConfig.direction.toUpperCase()}`;
      }

      const result = await conn.query(query);
      const rows = result.toArray();

      // Convert to CSV format
      const csvData = [
        columns,
        ...rows.map((row) =>
          columns.map((col) => {
            const value = (row as Record<string, unknown>)[col];
            return String(value ?? "");
          }),
        ),
      ];

      const csvContent = Papa.unparse(csvData);
      const blob = new Blob([csvContent], { type: "text/csv;charset=utf-8;" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "query_result.csv";
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error("Error downloading CSV:", err);
    }
  };

  const handleResizeStart = (colIdx: number, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setResizingColumn({
      index: colIdx,
      startX: e.clientX,
      startWidth: columnWidths[colIdx] || 150,
    });
  };

  const totalPages = Math.ceil(totalRows / pageSize);
  const numColumns = columns.length;
  const columnWidthsString = columnWidths
    .map((w: number) => `${w}px`)
    .join(" ");
  const gridTemplateColumns =
    columnWidths.length > 0
      ? `60px ${columnWidthsString}`
      : `60px repeat(${numColumns}, minmax(150px, 1fr))`;

  if (error) {
    return (
      <div className="p-4 text-red-600 border border-red-300 rounded">
        Error: {error}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full flex-1">
      {/* Table */}
      <div
        className="overflow-auto customScrollbar h-full min-h-0 font-mono text-xs"
        style={{ maxHeight }}
      >
        {isLoading ? (
          <div className="flex items-center justify-center p-8">
            <span className="text-muted-foreground">Loading...</span>
          </div>
        ) : (
          <div className="flex flex-col min-w-fit">
            {/* Fixed Header */}
            <div
              className="grid flex-shrink-0 border-b bg-muted sticky top-0 z-10"
              style={{ gridTemplateColumns }}
            >
              {/* Row number header */}
              <div className="h-8 px-3 flex items-center justify-center font-semibold uppercase border-r bg-muted/80" />

              {columns.map((col, idx) => {
                const isSorted = sortConfig.column === col;
                let sortIcon: React.ReactNode;

                if (isSorted && sortConfig.direction === "asc") {
                  sortIcon = <ChevronUp className="h-4 w-4" />;
                } else if (isSorted && sortConfig.direction === "desc") {
                  sortIcon = <ChevronDown className="h-4 w-4" />;
                } else {
                  sortIcon = <ChevronsUpDown className="h-4 w-4 opacity-30" />;
                }

                return (
                  <div
                    key={col}
                    className={`relative h-8 px-3 flex items-center font-semibold uppercase border-r last:border-r-0 overflow-hidden cursor-pointer ${
                      selectedCell?.row === 0 && selectedCell?.col === idx
                        ? "bg-primary/20 ring-2 ring-primary ring-inset"
                        : "hover:bg-muted-foreground/10"
                    }`}
                    onClick={() => {
                      setSelectedCell({ row: 0, col: idx });
                      handleSort(col);
                    }}
                    title={col}
                  >
                    <span className="truncate flex items-center gap-2">
                      {col}
                      {sortIcon}
                    </span>
                    <div
                      className="absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-primary/50 active:bg-primary"
                      onMouseDown={(e) => handleResizeStart(idx, e)}
                    />
                  </div>
                );
              })}
            </div>

            {/* Scrollable Body */}
            <div className="flex flex-col">
              {data.map((row, rowIdx) => (
                <div
                  key={rowIdx}
                  className="grid border-b"
                  style={{ gridTemplateColumns }}
                >
                  {/* Row number */}
                  <div className="h-7 px-3 py-1 flex items-center justify-center border-r bg-muted/30 text-muted-foreground">
                    {currentPage * pageSize + rowIdx + 1}
                  </div>

                  {row.map((cell, cellIdx) => (
                    <DataCell
                      key={cellIdx}
                      cell={String(cell ?? "")}
                      rowIdx={rowIdx}
                      cellIdx={cellIdx}
                      isSelected={
                        selectedCell?.row === rowIdx + 1 &&
                        selectedCell?.col === cellIdx
                      }
                      onClick={() =>
                        setSelectedCell({ row: rowIdx + 1, col: cellIdx })
                      }
                    />
                  ))}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Pagination */}
      <div className="flex items-center justify-between shrink-0 p-4 border-t">
        <div className="text-sm text-muted-foreground">
          Page {currentPage + 1} of {totalPages}
          {" · "}
          Showing {currentPage * pageSize + 1} -{" "}
          {Math.min((currentPage + 1) * pageSize, totalRows)} of {totalRows}
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleDownloadCsv}
            disabled={isLoading || !tableName}
          >
            <Download className="h-4 w-4 mr-2" />
            CSV
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setCurrentPage(0)}
            disabled={currentPage === 0 || isLoading}
          >
            First
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setCurrentPage((p) => Math.max(0, p - 1))}
            disabled={currentPage === 0 || isLoading}
          >
            Previous
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              setCurrentPage((p) => Math.min(totalPages - 1, p + 1))
            }
            disabled={currentPage >= totalPages - 1 || isLoading}
          >
            Next
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setCurrentPage(totalPages - 1)}
            disabled={currentPage >= totalPages - 1 || isLoading}
          >
            Last
          </Button>
        </div>
      </div>
    </div>
  );
};
