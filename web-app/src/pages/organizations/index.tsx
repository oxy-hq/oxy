import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useOrganizations } from "@/hooks/api/organizations/useOrganizations";
import { Input } from "@/components/ui/shadcn/input";
import { Search } from "lucide-react";
import ROUTES from "@/libs/utils/routes";
import Header from "./components/Header";
import Content from "./content";
import NewOrganizationDialog from "./new_organization_dialog";

export default function OrganizationsPage() {
  const navigate = useNavigate();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");

  const { data: orgsData, isLoading, error, refetch } = useOrganizations();

  const filteredOrganizations =
    orgsData?.organizations?.filter((org) =>
      org.name.toLowerCase().includes(searchQuery.toLowerCase()),
    ) || [];

  const handleOrganizationClick = (organizationId: string) => {
    navigate(ROUTES.ORG.PROJECTS(organizationId));
  };

  const handleClearSearch = () => setSearchQuery("");

  return (
    <div className="flex flex-col w-full">
      <Header />

      <div className="flex-1 p-6 max-w-6xl mx-auto max-w-[1200px] w-full">
        <div className="mb-6">
          <h1 className="text-2xl mb-4">Your Organizations</h1>
          <div className="flex items-center justify-between">
            <div className="flex items-center">
              {orgsData?.organizations && orgsData.organizations.length > 0 && (
                <div className="relative">
                  <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    placeholder="Search for an organization"
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="pl-10 w-80"
                  />
                </div>
              )}
            </div>
            <NewOrganizationDialog
              isOpen={isModalOpen}
              onOpenChange={setIsModalOpen}
            />
          </div>
        </div>

        <div className="space-y-6">
          <Content
            organizations={orgsData?.organizations}
            filteredOrganizations={filteredOrganizations}
            searchQuery={searchQuery}
            isLoading={isLoading}
            error={error}
            onOrganizationClick={handleOrganizationClick}
            onClearSearch={handleClearSearch}
            onRetry={refetch}
          />
        </div>
      </div>
    </div>
  );
}
