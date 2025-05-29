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
  const isMobile = useMediaQuery("(max-width: 767px)");
  const { width } = useWindowSize();

  const useIconOnly = width < 900;

  const maxVisiblePages = useMemo(() => {
    if (isMobile) return 3;
    if (width >= 1400) return 15;
    if (width >= 1200) return 12;
    if (width >= 1024) return 10;
    if (width >= 768) return 8;
    return 5;
  }, [isMobile, width]);

  const visiblePages = useMemo(() => {
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

    const getDesktopPages = () => {
      if (total_pages <= maxVisiblePages) {
        return Array.from({ length: total_pages }, (_, i) => i + 1);
      }
      if (total_pages === 2) {
        return [1, 2];
      }
      const pages: (number | "ellipsis")[] = [1];
      const siblings = Math.max(1, Math.floor((maxVisiblePages - 4) / 2));
      let left = Math.max(2, page - siblings);
      let right = Math.min(total_pages - 1, page + siblings);
      if (page - 1 <= siblings) {
        right = Math.min(total_pages - 1, right + (siblings - (page - 2)));
      }
      if (total_pages - page <= siblings) {
        left = Math.max(2, left - (siblings - (total_pages - page - 1)));
      }
      if (left > 2) pages.push("ellipsis");
      for (let i = left; i <= right; i++) {
        pages.push(i);
      }
      if (right < total_pages - 1) pages.push("ellipsis");
      pages.push(total_pages);
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
      <div className={`flex-shrink-0 ${isMobile ? "flex justify-center" : ""}`}>
        {onLimitChange && currentLimit && (
          <ItemsPerPageFilter
            currentLimit={currentLimit}
            onLimitChange={onLimitChange}
            isLoading={isLoading}
          />
        )}
      </div>
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
