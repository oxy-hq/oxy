import React from "react";
import { TableRow, TableCell } from "@/components/ui/shadcn/table";
import { Button } from "@/components/ui/shadcn/button";
import { RefreshCw, Loader2 } from "lucide-react";

interface TableRowWrapperProps {
  colSpan: number;
}

const TableRowWrapper: React.FC<
  React.PropsWithChildren<TableRowWrapperProps>
> = ({ colSpan, children }) => {
  return (
    <TableRow>
      <TableCell colSpan={colSpan} className="text-center py-12">
        <div className="flex flex-col items-center space-y-1">{children}</div>
      </TableCell>
    </TableRow>
  );
};

interface Props {
  isEmpty: boolean;
  loading: boolean;
  colSpan: number;
  noFoundTitle?: string;
  noFoundDescription?: string;
  error?: string;
  onRetry?: () => void;
}

const TableContentWrapper: React.FC<React.PropsWithChildren<Props>> = ({
  isEmpty,
  loading,
  colSpan,
  children,
  noFoundTitle = "No found",
  noFoundDescription,
  error,
  onRetry,
}) => {
  if (loading) {
    return (
      <TableRowWrapper colSpan={colSpan}>
        <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
        <p className="text-sm text-muted-foreground">Loading...</p>
      </TableRowWrapper>
    );
  }

  if (error) {
    return (
      <TableRowWrapper colSpan={colSpan}>
        <h3 className="text-lg font-medium text-destructive">Error</h3>
        <p className="text-sm text-muted-foreground max-w-md">{error}</p>
        {onRetry && (
          <Button
            onClick={onRetry}
            variant="outline"
            size="sm"
            className="mt-2"
          >
            <RefreshCw />
            Try Again
          </Button>
        )}
      </TableRowWrapper>
    );
  }

  if (isEmpty) {
    return (
      <TableRowWrapper colSpan={colSpan}>
        <h3 className="text-lg font-medium text-foreground">{noFoundTitle}</h3>
        {noFoundDescription && (
          <p className="text-sm text-muted-foreground max-w-md">
            {noFoundDescription}
          </p>
        )}
      </TableRowWrapper>
    );
  }

  return children;
};

export default TableContentWrapper;
