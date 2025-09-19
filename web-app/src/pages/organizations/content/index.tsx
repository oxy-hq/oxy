import EmptyState from "./EmptyState";
import LoadingState from "./LoadingState";
import OrganizationCard from "./OrganizationCard";

interface Organization {
  id: string;
  name: string;
  role: string;
  created_at: string;
}

interface Props {
  organizations?: Organization[];
  filteredOrganizations: Organization[];
  searchQuery: string;
  isLoading: boolean;
  error: Error | null;
  onOrganizationClick: (organizationId: string) => void;
  onClearSearch: () => void;
  onRetry: () => void;
}

const Content = ({
  organizations,
  filteredOrganizations,
  searchQuery,
  isLoading,
  error,
  onOrganizationClick,
  onClearSearch,
  onRetry,
}: Props) => {
  if (isLoading) {
    return <LoadingState />;
  }

  if (error) {
    return <LoadingState error={error} onRetry={onRetry} />;
  }

  if (!organizations || organizations.length === 0) {
    return <EmptyState type="no-organizations" />;
  }

  if (filteredOrganizations.length === 0 && searchQuery.trim()) {
    return (
      <EmptyState type="no-search-results" onClearSearch={onClearSearch} />
    );
  }

  return (
    <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
      {filteredOrganizations.map((org) => (
        <OrganizationCard
          key={org.id}
          organization={org}
          onOrganizationClick={onOrganizationClick}
        />
      ))}
    </div>
  );
};

export default Content;
