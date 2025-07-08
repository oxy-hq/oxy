import { Button } from "@/components/ui/shadcn/button";

interface EmptyStateProps {
  hasSearchQuery: boolean;
  onClearSearch: () => void;
}

const EmptyState = ({ hasSearchQuery, onClearSearch }: EmptyStateProps) => (
  <div className="text-center py-12">
    <p className="text-gray-500 dark:text-gray-400">
      {hasSearchQuery
        ? "No repositories match your search."
        : "No repositories found."}
    </p>
    {hasSearchQuery && (
      <Button variant="outline" onClick={onClearSearch} className="mt-4">
        Clear Search
      </Button>
    )}
  </div>
);

export default EmptyState;
