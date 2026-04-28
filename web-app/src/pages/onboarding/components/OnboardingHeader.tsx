import { ChevronLeft, LogOut } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { useAuth } from "@/contexts/AuthContext";
import { useOrgs } from "@/hooks/api/organizations";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";
import useTheme from "@/stores/useTheme";

export default function OnboardingHeader() {
  const navigate = useNavigate();
  const { theme } = useTheme();
  const { logout } = useAuth();
  const { data: currentUser } = useCurrentUser();
  const { data: orgs } = useOrgs();
  const hasAnyWorkspace = !!orgs?.some((o) => (o.workspace_count ?? 0) > 0);

  return (
    <div className='flex items-center justify-between gap-2 p-6 font-medium'>
      {hasAnyWorkspace ? (
        <Button
          variant='ghost'
          size='sm'
          onClick={() => navigate(ROUTES.ROOT)}
          className='-ml-2 gap-1 text-muted-foreground hover:text-foreground'
        >
          <ChevronLeft className='size-4' />
          Back to Oxygen
        </Button>
      ) : (
        <div className='flex items-center gap-2'>
          <img src={theme === "dark" ? "/oxygen-dark.svg" : "/oxygen-light.svg"} alt='Oxygen' />
          <span className='truncate text-sm'>Oxygen</span>
        </div>
      )}
      {currentUser?.email && (
        <div className='group relative'>
          <div className='flex cursor-pointer flex-col items-end text-right leading-tight'>
            <span className='text-muted-foreground text-xs'>Logged in as</span>
            <span className='truncate font-normal text-sm'>{currentUser.email}</span>
          </div>
          <div className='pointer-events-none absolute top-full right-0 z-10 pt-2 opacity-0 transition-opacity focus-within:pointer-events-auto focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100'>
            <Button
              variant='ghost'
              size='sm'
              onClick={logout}
              aria-label='Log out'
              className='gap-1.5 shadow-sm'
            >
              <LogOut className='size-3.5' />
              Log out
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
