import { Building2, Search } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent } from "@/components/ui/shadcn/card";

interface EmptyStateProps {
  type: "no-workspaces" | "no-search-results";
  onClearSearch?: () => void;
}

const EmptyState = ({ type, onClearSearch }: EmptyStateProps) => {
  if (type === "no-search-results") {
    return (
      <Card>
        <CardContent className='flex flex-col items-center justify-center py-12'>
          <Search className='mb-4 h-12 w-12 text-muted-foreground' />
          <h3 className='mb-2 font-semibold text-lg'>No workspaces found</h3>
          <p className='mb-4 text-center text-muted-foreground'>
            No workspaces match your search criteria. Try adjusting your search terms.
          </p>
          {onClearSearch && (
            <Button variant='outline' onClick={onClearSearch}>
              Clear search
            </Button>
          )}
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardContent className='flex flex-col items-center justify-center py-12'>
        <Building2 className='mb-4 h-12 w-12 text-muted-foreground' />
        <h3 className='mb-2 font-semibold text-lg'>No workspaces yet</h3>
        <p className='mb-4 text-center text-muted-foreground'>
          Create your first workspace to start collaborating with your team.
        </p>
      </CardContent>
    </Card>
  );
};

export default EmptyState;
