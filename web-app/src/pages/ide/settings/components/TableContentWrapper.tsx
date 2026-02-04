import { Loader2, RefreshCw } from "lucide-react";
import type React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";

interface TableRowWrapperProps {
  colSpan: number;
}

const TableRowWrapper: React.FC<React.PropsWithChildren<TableRowWrapperProps>> = ({
  colSpan,
  children
}) => {
  return (
    <TableRow>
      <TableCell colSpan={colSpan} className='py-12 text-center'>
        <div className='flex flex-col items-center space-y-1'>{children}</div>
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
  onRetry
}) => {
  if (loading) {
    return (
      <TableRowWrapper colSpan={colSpan}>
        <Loader2 className='h-8 w-8 animate-spin text-muted-foreground' />
        <p className='text-muted-foreground text-sm'>Loading...</p>
      </TableRowWrapper>
    );
  }

  if (error) {
    return (
      <TableRowWrapper colSpan={colSpan}>
        <h3 className='font-medium text-destructive text-lg'>Error</h3>
        <p className='max-w-md text-muted-foreground text-sm'>{error}</p>
        {onRetry && (
          <Button onClick={onRetry} variant='outline' size='sm' className='mt-2'>
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
        <h3 className='font-medium text-foreground text-lg'>{noFoundTitle}</h3>
        {noFoundDescription && (
          <p className='max-w-md text-muted-foreground text-sm'>{noFoundDescription}</p>
        )}
      </TableRowWrapper>
    );
  }

  return children;
};

export default TableContentWrapper;
