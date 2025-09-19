import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Badge } from "@/components/ui/shadcn/badge";
import { Building2, Users } from "lucide-react";

interface Organization {
  id: string;
  name: string;
  role: string;
  created_at: string;
}

interface OrganizationCardProps {
  organization: Organization;
  onOrganizationClick: (organizationId: string) => void;
}

const OrganizationCard = ({
  organization,
  onOrganizationClick,
}: OrganizationCardProps) => {
  return (
    <Card
      className="hover:shadow-md transition-shadow cursor-pointer group"
      onClick={() => onOrganizationClick(organization.id)}
    >
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-2">
            <Building2 className="h-5 w-5 text-primary" />
            <CardTitle className="text-lg group-hover:text-primary transition-colors">
              {organization.name}
            </CardTitle>
          </div>
          <Badge
            variant={organization.role === "owner" ? "default" : "secondary"}
          >
            {organization.role}
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-2 text-sm text-muted-foreground">
          <div className="flex items-center gap-2">
            <Users className="h-4 w-4" />
            <span>
              Member since{" "}
              {new Date(organization.created_at).toLocaleDateString()}
            </span>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};

export default OrganizationCard;
