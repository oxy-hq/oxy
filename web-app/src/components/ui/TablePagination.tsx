import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink
} from "@/components/ui/shadcn/pagination";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import { cn } from "@/libs/shadcn/utils";

interface TablePaginationProps {
  currentPage: number;
  totalPages: number;
  totalItems: number;
  pageSize: number;
  onPageChange: (page: number) => void;
  itemLabel?: string;
}

export default function TablePagination({
  currentPage,
  totalPages,
  totalItems,
  pageSize,
  onPageChange,
  itemLabel = "items"
}: TablePaginationProps) {
  const offset = (currentPage - 1) * pageSize;
  const hasNextPage = currentPage < totalPages;
  const hasPrevPage = currentPage > 1;

  if (totalPages <= 1) {
    return null;
  }

  return (
    <div className='flex items-center justify-between border-t pt-4'>
      <div className='text-muted-foreground text-sm'>
        Showing {offset + 1}–{Math.min(offset + pageSize, totalItems)} of {totalItems} {itemLabel}
      </div>
      <Pagination className='w-auto flex-1 justify-end'>
        <PaginationContent className='flex-wrap justify-center'>
          <PaginationItem>
            <Button
              variant='ghost'
              disabled={!hasPrevPage}
              onClick={(e) => {
                e.preventDefault();
                if (hasPrevPage) onPageChange(currentPage - 1);
              }}
            >
              <ChevronLeft />
            </Button>
          </PaginationItem>
          {generatePaginationItems(currentPage, totalPages).map((pageNum, idx) => (
            <PaginationItem key={pageNum === "ellipsis" ? `ellipsis-${idx}` : pageNum}>
              {pageNum === "ellipsis" ? (
                <PaginationEllipsis />
              ) : (
                <PaginationLink
                  href='#'
                  onClick={(e) => {
                    e.preventDefault();
                    onPageChange(pageNum);
                  }}
                  isActive={pageNum === currentPage}
                  className={cn(
                    buttonVariants({
                      variant: pageNum === currentPage ? "default" : "outline"
                    })
                  )}
                >
                  {pageNum}
                </PaginationLink>
              )}
            </PaginationItem>
          ))}
          <PaginationItem>
            <Button
              variant='ghost'
              disabled={!hasNextPage}
              onClick={(e) => {
                e.preventDefault();
                if (hasNextPage) onPageChange(currentPage + 1);
              }}
            >
              <ChevronRight />
            </Button>
          </PaginationItem>
        </PaginationContent>
      </Pagination>
    </div>
  );
}

// Helper function to generate pagination items with ellipsis
function generatePaginationItems(currentPage: number, totalPages: number): (number | "ellipsis")[] {
  const items: (number | "ellipsis")[] = [];

  if (totalPages <= 7) {
    // Show all pages if total is 7 or less
    for (let i = 1; i <= totalPages; i++) {
      items.push(i);
    }
  } else {
    // Always show first page
    items.push(1);

    if (currentPage > 3) {
      items.push("ellipsis");
    }

    // Show pages around current page
    const start = Math.max(2, currentPage - 1);
    const end = Math.min(totalPages - 1, currentPage + 1);

    for (let i = start; i <= end; i++) {
      items.push(i);
    }

    if (currentPage < totalPages - 2) {
      items.push("ellipsis");
    }

    // Always show last page
    items.push(totalPages);
  }

  return items;
}
