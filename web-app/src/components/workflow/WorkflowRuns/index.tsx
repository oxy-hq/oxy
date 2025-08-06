// Render the list of workflow runs
import { useListWorkflowRuns } from "../useWorkflowRun";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Link } from "react-router-dom";
import { DataTablePagination } from "./Pagination";
import {
  ColumnDef,
  flexRender,
  getCoreRowModel,
  PaginationState,
  Row,
  useReactTable,
} from "@tanstack/react-table";
import { RunInfo } from "@/services/types";
import { useRef, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { RotateCcw, XIcon } from "lucide-react";

export const WorkflowRuns = ({
  workflowId,
  onClose,
}: {
  workflowId: string;
  onClose?: () => void;
}) => {
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 50,
  });
  const { data, isLoading, refetch } = useListWorkflowRuns(
    workflowId,
    pagination,
  );
  const columns: ColumnDef<RunInfo>[] = useRef([
    {
      accessorKey: "run_index",
      header: "Run ID",
      cell: ({ row }: { row: Row<RunInfo> }) => (
        <Link
          className="text-blue-600 hover:underline"
          to={`/workflows/${btoa(row.original.source_id)}/runs/${row.original.run_index}`}
        >
          {row.original.run_index}
        </Link>
      ),
    },
    {
      accessorKey: "status",
      header: "Status",
    },
    {
      accessorKey: "created_at",
      header: "Created At",
      cell: ({ row }: { row: Row<RunInfo> }) => {
        const createdAt = row.original.created_at;
        return createdAt ? new Date(createdAt).toLocaleString() : "";
      },
    },
    {
      accessorKey: "updated_at",
      header: "Ended At",
      cell: ({ row }: { row: Row<RunInfo> }) => {
        const isFinished =
          row.original.status === "completed" ||
          row.original.status === "failed";
        const updatedAt = row.original.updated_at;
        return isFinished && updatedAt
          ? new Date(updatedAt).toLocaleString()
          : "";
      },
    },
  ]).current;
  const items = data?.items;
  const table = useReactTable({
    data: items || [],
    columns,
    getCoreRowModel: getCoreRowModel(),
    state: {
      pagination,
    },
    onPaginationChange: setPagination,
    manualPagination: true,
    pageCount: data?.pagination?.num_pages || 1,
  });

  return (
    <div className="flex flex-col p-6 h-full">
      <div className="flex justify-between items-center mb-6">
        <h2 className="text-lg font-semibold">Workflow Runs</h2>
        <div className="flex items-center space-x-2">
          <Button
            variant="outline"
            tooltip={"Refresh Runs"}
            onClick={() => refetch()}
          >
            <RotateCcw className="w-4 h-4" />
          </Button>
          <Button
            variant="outline"
            tooltip={"Close"}
            onClick={() => onClose?.()}
          >
            <XIcon className="w-4 h-4" />
          </Button>
        </div>
      </div>
      <Table
        className="w-full"
        containerClassName="border rounded-lg overflow-y-auto flex-1 mb-4 customScrollbar"
      >
        <TableHeader className="sticky top-0 z-10 bg-background">
          {table.getHeaderGroups().map((headerGroup) => (
            <TableRow key={headerGroup.id}>
              {headerGroup.headers.map((header) => {
                return (
                  <TableHead key={header.id}>
                    {header.isPlaceholder
                      ? null
                      : flexRender(
                          header.column.columnDef.header,
                          header.getContext(),
                        )}
                  </TableHead>
                );
              })}
            </TableRow>
          ))}
        </TableHeader>

        <TableBody>
          {/* Render loading state */}
          {isLoading && (
            <TableRow>
              <TableCell colSpan={4}>
                <Skeleton className="h-4 w-full" />
              </TableCell>
            </TableRow>
          )}
          {/* Render runs */}
          {table.getRowModel().rows?.length ? (
            table.getRowModel().rows.map((row) => (
              <TableRow
                key={row.id}
                data-state={row.getIsSelected() && "selected"}
              >
                {row.getVisibleCells().map((cell) => (
                  <TableCell key={cell.id}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell colSpan={columns.length} className="h-24 text-center">
                No results.
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
      <DataTablePagination table={table} />
    </div>
  );
};
