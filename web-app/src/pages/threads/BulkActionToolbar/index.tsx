import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Checkbox } from "@/components/ui/checkbox";
import { X } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import DeleteAction from "./DeleteAction";
import useDeleteAllThread from "@/hooks/api/useDeleteAllThread";
import useBulkDeleteThreads from "@/hooks/api/useBulkDeleteThreads";

interface BulkActionToolbarProps {
  totalOnPage: number;
  totalAcrossAll?: number;
  isSelectAllPages: boolean;
  selectedThreads: Set<string>;
  onSelectAll: (checked: boolean) => void;
  onSelectAllPages: (checked: boolean) => void;
  onClearSelection: () => void;
}

const BulkActionToolbar: React.FC<BulkActionToolbarProps> = (props) => {
  const {
    totalOnPage,
    totalAcrossAll,
    isSelectAllPages,
    selectedThreads,
    onSelectAll,
    onSelectAllPages,
    onClearSelection,
  } = props;

  const selectedCount = selectedThreads.size;
  const isAllSelectedOnPage = selectedCount === totalOnPage && totalOnPage > 0;

  const { mutate: deleteAllThreads, isPending: isDeletingAll } =
    useDeleteAllThread();
  const { mutate: deleteThreads, isPending: isDeletingThreads } =
    useBulkDeleteThreads();
  const isDeleting = isDeletingAll || isDeletingThreads;

  const getSelectionLabel = () => {
    if (isSelectAllPages && totalAcrossAll) {
      return `All ${totalAcrossAll} threads selected`;
    }
    const threadText = selectedCount === 1 ? "thread" : "threads";
    return `${selectedCount} ${threadText} selected`;
  };

  const handleBulkDelete = () => {
    if (isSelectAllPages) {
      deleteAllThreads(undefined, {
        onSuccess: onClearSelection,
      });
    } else {
      const threadIds = Array.from(selectedThreads);
      deleteThreads(threadIds, {
        onSuccess: onClearSelection,
      });
    }
  };

  const shouldShowSelectAllAction =
    isAllSelectedOnPage &&
    !isSelectAllPages &&
    totalAcrossAll &&
    totalAcrossAll > totalOnPage;

  return (
    <div className="max-w-page-content mx-auto w-full px-2">
      <div className={cn("flex items-center gap-4 p-4 rounded-lg border")}>
        <div className="flex items-center gap-3">
          <Checkbox
            checked={isAllSelectedOnPage}
            onCheckedChange={onSelectAll}
            disabled={isDeleting}
          />
          <span className="text-sm font-medium">{getSelectionLabel()}</span>
        </div>

        {shouldShowSelectAllAction && (
          <Button
            variant="link"
            size="sm"
            onClick={() => onSelectAllPages(true)}
            disabled={isDeleting}
            className="h-auto p-0 text-sm underline"
          >
            Select all {totalAcrossAll} threads
          </Button>
        )}

        <div className="flex items-center gap-2 ml-auto">
          <Button
            variant="outline"
            size="sm"
            onClick={onClearSelection}
            disabled={isDeleting}
          >
            <X className="h-4 w-4" />
            Clear
          </Button>
          <DeleteAction
            selectedCount={selectedCount}
            isLoading={isDeleting}
            totalAcrossAll={totalAcrossAll}
            isSelectAllPages={isSelectAllPages}
            onBulkDelete={handleBulkDelete}
          />
        </div>
      </div>
    </div>
  );
};

export default BulkActionToolbar;
