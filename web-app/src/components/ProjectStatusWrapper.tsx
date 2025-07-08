import React from "react";
import { Navigate, useLocation } from "react-router-dom";
import { Loader2 } from "lucide-react";
import { ConfigErrorScreen } from "./ConfigErrorScreen";
import { useProjectStatus } from "@/hooks/useProjectStatus";

interface ProjectStatusWrapperProps {
  children: React.ReactNode;
}

const ProjectStatusWrapper = ({ children }: ProjectStatusWrapperProps) => {
  const { data: projectStatus, isLoading, error, refetch } = useProjectStatus();
  const location = useLocation();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <div className="text-center">
          <Loader2 className="animate-spin h-6 w-6 mx-auto mb-2" />
        </div>
      </div>
    );
  }

  // If there's an error checking project status, assume we need onboarding
  if (error || !projectStatus) {
    return <Navigate to="/onboarding" replace />;
  }

  // If project is in readonly mode, handle additional checks
  if (projectStatus.is_readonly) {
    if (!projectStatus.is_onboarded) {
      // If GitHub is not connected, redirect to onboarding
      return <Navigate to="/onboarding" replace />;
    }

    // Allow access to GitHub settings page even when config is invalid
    const isOnGitHubSettingsPage = location.pathname === "/github-settings";

    // Show error screen if config is invalid, but allow GitHub settings access
    if (!projectStatus.is_config_valid && !isOnGitHubSettingsPage) {
      return <ConfigErrorScreen onRetry={() => refetch()} />;
    }

    // If there are required secrets, navigate to secrets setup page
    if (
      projectStatus.required_secrets &&
      projectStatus.required_secrets.length > 0
    ) {
      return <Navigate to="/secrets/setup" replace />;
    }
  }

  // Project is set up correctly, render the main app
  return <>{children}</>;
};

export default ProjectStatusWrapper;
