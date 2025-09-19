import { useNavigate, useParams } from "react-router-dom";
import useTheme from "@/stores/useTheme";
import ROUTES from "@/libs/utils/routes";
import { useOrganizations } from "@/hooks/api/organizations/useOrganizations";

const Header = () => {
  const { organizationId } = useParams<{ organizationId: string }>();
  const navigate = useNavigate();
  const { theme } = useTheme();
  const { data: orgsData } = useOrganizations();

  const currentOrganization = orgsData?.organizations?.find(
    (org) => org.id === organizationId,
  );

  return (
    <div className="border-b border-border p-4 mb-6">
      <div className="flex items-center gap-2">
        <img
          width={24}
          height={24}
          src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
          alt="Oxy"
          className="cursor-pointer"
          onClick={() => navigate(ROUTES.ORG.ROOT)}
        />
        <div className="text-border">
          <svg
            viewBox="0 0 24 24"
            width="16"
            height="16"
            stroke="currentColor"
            strokeWidth="1"
            strokeLinecap="round"
            strokeLinejoin="round"
            fill="none"
            shapeRendering="geometricPrecision"
          >
            <path d="M16 3.549L7.12 20.600" />
          </svg>
        </div>
        <p
          className="text-sm cursor-pointer hover:text-primary transition-colors"
          onClick={() => navigate(ROUTES.ORG.ROOT)}
        >
          {currentOrganization?.name || "Organization"}
        </p>
      </div>
    </div>
  );
};

export default Header;
