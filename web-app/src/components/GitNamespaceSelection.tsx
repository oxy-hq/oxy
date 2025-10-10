import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Github, Loader2, Plus } from "lucide-react";
import {
  useGitHubInstallAppUrl,
  useGitHubNamespaces,
} from "@/hooks/api/github";
import { openGitHubAppInstallation } from "@/utils/githubAppInstall";

interface Props {
  value?: string;
  onChange?: (value: string) => void;
}

export const GitNamespaceSelection = ({ value, onChange }: Props) => {
  const { data: gitNamespaces = [], isPending: isLoadingNamespaces } =
    useGitHubNamespaces();

  const { data: installAppUrl, isPending: isLoadingInstallApp } =
    useGitHubInstallAppUrl();

  const isLoading = isLoadingNamespaces || isLoadingInstallApp;

  const handleInstallApp = async () => {
    if (installAppUrl) {
      try {
        await openGitHubAppInstallation(installAppUrl);
      } catch (error) {
        console.error("Error opening GitHub App installation:", error);
      }
    }
  };

  const handleOnChange = (selectedValue: string) => {
    if (selectedValue === "add-new-namespace") {
      handleInstallApp();
    } else {
      onChange?.(selectedValue);
    }
  };

  return (
    <div className="space-y-2">
      <Label htmlFor="git-namespace">Git Scope</Label>
      <Select value={value} onValueChange={handleOnChange}>
        <SelectTrigger>
          <SelectValue placeholder="Select Git Scope" />
        </SelectTrigger>
        <SelectContent>
          {(() => {
            if (isLoading) {
              return (
                <SelectItem value="loading" disabled>
                  <div className="flex items-center gap-2">
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Loading...
                  </div>
                </SelectItem>
              );
            }

            const options = [];

            if (gitNamespaces.length > 0) {
              options.push(
                ...gitNamespaces.map((namespace) => (
                  <SelectItem key={namespace.id} value={namespace.id}>
                    <Github className="h-4 w-4" />
                    {namespace.name}
                  </SelectItem>
                )),
              );
            }

            options.push(
              <SelectItem key={"add-new-namespace"} value={"add-new-namespace"}>
                <Plus className="h-4 w-4" />
                Add New Namespace
              </SelectItem>,
            );

            return options;
          })()}
        </SelectContent>
      </Select>
    </div>
  );
};
