import { useParams, Link } from "react-router-dom";
import { ChevronLeft } from "lucide-react";
import {
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/shadcn/breadcrumb";
import PageHeader from "@/components/PageHeader";
import { Breadcrumb } from "@/components/ui/shadcn/breadcrumb";
import { Button } from "@/components/ui/shadcn/button";
import useAgent from "@/hooks/api/useAgent";
import { getAgentNameFromPath } from "@/libs/utils/string";

const Header: React.FC = () => {
  const pathb64 = useParams<{ pathb64: string }>().pathb64!;
  const agentPath = atob(pathb64);
  const { data: agent } = useAgent(pathb64);

  return (
    <PageHeader className="flex-col border-b border-border w-full">
      <div className="flex items-center justify-between py-[2px]">
        <Link to="/">
          <Button variant="ghost">
            <ChevronLeft className="w-4 h-4" />
            Return to home
          </Button>
        </Link>

        <Breadcrumb>
          <BreadcrumbList>
            <BreadcrumbItem>
              <BreadcrumbLink asChild>
                <Link to="/">Agents</Link>
              </BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator />
            <BreadcrumbItem>
              <BreadcrumbPage>
                {agent?.name || getAgentNameFromPath(agentPath)}
              </BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>

        <div className="flex items-center gap-2" />
      </div>
    </PageHeader>
  );
};

export default Header;
