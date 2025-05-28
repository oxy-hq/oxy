import React, { useMemo } from "react";
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationLink,
  PaginationEllipsis,
} from "@/components/ui/shadcn/pagination";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { PaginationInfo } from "@/types/chat";
import { useMediaQuery, useWindowSize } from "usehooks-ts";
import { cn } from "@/libs/shadcn/utils";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import ItemsPerPageFilter from "./ItemsPerPageFilter";

interface ThreadsPaginationProps {
  pagination: PaginationInfo;
  onPageChange: (page: number) => void;
  onLimitChange?: (limit: number) => void;
  currentLimit?: number;
  isLoading?: boolean;
}

export type { ThreadsPaginationProps };

const ResponsivePaginationPrevious = ({
  className,
  showText,
  ...props
}: React.ComponentProps<"a"> & { showText: boolean }) => (
  <a
    aria-label="Go to previous page"
    className={cn(
      buttonVariants({ variant: "ghost", size: showText ? "default" : "icon" }),
      "gap-1 pl-2.5 min-w-0",
      className,
    )}
    {...props}
  >
    <ChevronLeft className="h-4 w-4 shrink-0" />
    {showText && <span>Previous</span>}
    <span className="sr-only">Previous</span>
  </a>
);

const ResponsivePaginationNext = ({
  className,
  showText,
  ...props
}: React.ComponentProps<"a"> & { showText: boolean }) => (
  <a
    aria-label="Go to next page"
    className={cn(
      buttonVariants({ variant: "ghost", size: showText ? "default" : "icon" }),
      "gap-1 pr-2.5 min-w-0",
      className,
    )}
    {...props}
  >
    {showText && <span>Next</span>}
    <span className="sr-only">Next</span>
    <ChevronRight className="h-4 w-4 shrink-0" />
  </a>
);

const ThreadsPagination: React.FC<ThreadsPaginationProps> = ({
  pagination,
  onPageChange,
  onLimitChange,
  currentLimit,
  isLoading = false,
}) => {
  const { page, total_pages, has_previous, has_next } = pagination;
  const isMobile = useMediaQuery("(max-width: 767px)"); // 768px is the mobile breakpoint
  const { width } = useWindowSize();

  // Use icon-only buttons for smaller screens (including small tablets)
  const useIconOnly = width < 900; // Hide text below 900px instead of just mobile breakpoint

  // Calculate how many pages to show based on viewport width (memoized for performance)
  const maxVisiblePages = useMemo(() => {
    if (isMobile) return 3;

    // Responsive breakpoints for different screen sizes
    if (width >= 1400) return 15; // Extra large screens
    if (width >= 1200) return 12; // Large screens
    if (width >= 1024) return 10; // Medium-large screens
    if (width >= 768) return 8; // Medium screens
    return 5; // Small screens (but not mobile)
  }, [isMobile, width]);

  // Generate page numbers to show (memoized for performance)
  const visiblePages = useMemo(() => {
    // Mobile pagination logic - keep simple for small screens
    const getMobilePages = () => {
      if (total_pages <= 3) {
        return Array.from({ length: total_pages }, (_, i) => i + 1);
      }
      if (page <= 2) {
        return [1, 2, "ellipsis", total_pages];
      }
      if (page >= total_pages - 1) {
        return [1, "ellipsis", total_pages - 1, total_pages];
      }
      return [1, "ellipsis", page, "ellipsis", total_pages];
    };

    // Desktop pagination logic - balanced first/last approach
    const getDesktopPages = () => {
      if (total_pages <= maxVisiblePages) {
        return Array.from({ length: total_pages }, (_, i) => i + 1);
      }

      const pages: (number | "ellipsis")[] = [];

      // Determine how many pages to show on each side
      const firstCount = Math.min(5, Math.floor(maxVisiblePages * 0.4));
      const lastCount = Math.min(5, Math.floor(maxVisiblePages * 0.4));

      // Add first pages (1, 2, 3, 4, 5)
      for (let i = 1; i <= Math.min(firstCount, total_pages); i++) {
        pages.push(i);
      }

      // If current page is within first or last section, no need for middle logic
      const isInFirstSection = page <= firstCount;
      const isInLastSection = page > total_pages - lastCount;

      if (!isInFirstSection && !isInLastSection) {
        // Current page is in the middle - add ellipsis, current page area, ellipsis
        pages.push("ellipsis");

        // Add current page and immediate neighbors
        const neighborsToShow = Math.floor(
          (maxVisiblePages - firstCount - lastCount - 2) / 2,
        ); // -2 for ellipses
        const start = Math.max(firstCount + 1, page - neighborsToShow);
        const end = Math.min(total_pages - lastCount, page + neighborsToShow);

        for (let i = start; i <= end; i++) {
          pages.push(i);
        }

        pages.push("ellipsis");
      } else if (firstCount < total_pages - lastCount) {
        // Add single ellipsis if there's a gap
        pages.push("ellipsis");
      }

      // Add last pages (..., 96, 97, 98, 99, 100)
      const lastStart = Math.max(firstCount + 1, total_pages - lastCount + 1);
      for (let i = lastStart; i <= total_pages; i++) {
        if (!pages.includes(i)) {
          pages.push(i);
        }
      }

      return pages;
    };

    return isMobile ? getMobilePages() : getDesktopPages();
  }, [isMobile, page, total_pages, maxVisiblePages]);

  if (total_pages <= 1 && !onLimitChange) {
    return null;
  }

  return (
    <div
      className={`flex ${isMobile ? "flex-col gap-4" : "items-center justify-between"}`}
    >
      {/* Left side - Items per page filter */}
      <div className={`flex-shrink-0 ${isMobile ? "flex justify-center" : ""}`}>
        {onLimitChange && currentLimit && (
          <ItemsPerPageFilter
            currentLimit={currentLimit}
            onLimitChange={onLimitChange}
            isLoading={isLoading}
          />
        )}
      </div>

      {/* Right side - Pagination */}
      <div className={`flex-shrink-0 ${isMobile ? "flex justify-center" : ""}`}>
        {total_pages > 1 && (
          <Pagination>
            <PaginationContent className="flex-wrap justify-center">
              <PaginationItem>
                <ResponsivePaginationPrevious
                  href="#"
                  showText={!useIconOnly}
                  onClick={(e) => {
                    e.preventDefault();
                    if (has_previous && !isLoading) {
                      onPageChange(page - 1);
                    }
                  }}
                  className={
                    !has_previous || isLoading
                      ? "pointer-events-none opacity-50"
                      : ""
                  }
                />
              </PaginationItem>

              {visiblePages.map((pageNum, index) => (
                <PaginationItem
                  key={pageNum === "ellipsis" ? `ellipsis-${index}` : pageNum}
                >
                  {pageNum === "ellipsis" ? (
                    <PaginationEllipsis />
                  ) : (
                    <PaginationLink
                      href="#"
                      onClick={(e) => {
                        e.preventDefault();
                        if (!isLoading && typeof pageNum === "number") {
                          onPageChange(pageNum);
                        }
                      }}
                      isActive={pageNum === page}
                      className={
                        isLoading ? "pointer-events-none opacity-50" : ""
                      }
                    >
                      {pageNum}
                    </PaginationLink>
                  )}
                </PaginationItem>
              ))}

              <PaginationItem>
                <ResponsivePaginationNext
                  href="#"
                  showText={!useIconOnly}
                  onClick={(e) => {
                    e.preventDefault();
                    if (has_next && !isLoading) {
                      onPageChange(page + 1);
                    }
                  }}
                  className={
                    !has_next || isLoading
                      ? "pointer-events-none opacity-50"
                      : ""
                  }
                />
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        )}
      </div>
    </div>
  );
};

export default ThreadsPagination;
