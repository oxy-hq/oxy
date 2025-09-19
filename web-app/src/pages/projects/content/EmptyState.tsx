import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent } from "@/components/ui/shadcn/card";
import { FolderOpen, Search } from "lucide-react";

interface EmptyStateProps {
  type: "no-projects" | "no-search-results";
  onClearSearch?: () => void;
}

const EmptyState = ({ type, onClearSearch }: EmptyStateProps) => {
  if (type === "no-search-results") {
    return (
      <Card>
        <CardContent className="flex flex-col items-center justify-center py-12">
          <Search className="h-12 w-12 text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">No projects found</h3>
          <p className="text-muted-foreground text-center mb-4">
            No projects match your search criteria. Try adjusting your search
            terms.
          </p>
          {onClearSearch && (
            <Button variant="outline" onClick={onClearSearch}>
              Clear search
            </Button>
          )}
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardContent className="flex flex-col items-center justify-center py-12">
        <FolderOpen className="h-12 w-12 text-muted-foreground mb-4" />
        <h3 className="text-lg font-semibold mb-2">No projects yet</h3>
        <p className="text-muted-foreground text-center mb-4">
          Create your first project to start building and deploying your
          applications.
        </p>
      </CardContent>
    </Card>
  );
};

export default EmptyState;
