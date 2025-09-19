import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent } from "@/components/ui/shadcn/card";
import { Building2, Search } from "lucide-react";

interface EmptyStateProps {
  type: "no-organizations" | "no-search-results";
  onClearSearch?: () => void;
}

const EmptyState = ({ type, onClearSearch }: EmptyStateProps) => {
  if (type === "no-search-results") {
    return (
      <Card>
        <CardContent className="flex flex-col items-center justify-center py-12">
          <Search className="h-12 w-12 text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">No organizations found</h3>
          <p className="text-muted-foreground text-center mb-4">
            No organizations match your search criteria. Try adjusting your
            search terms.
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
        <Building2 className="h-12 w-12 text-muted-foreground mb-4" />
        <h3 className="text-lg font-semibold mb-2">No organizations yet</h3>
        <p className="text-muted-foreground text-center mb-4">
          Create your first organization to start collaborating with your team.
        </p>
      </CardContent>
    </Card>
  );
};

export default EmptyState;
