import React, { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Eye, EyeOff } from "lucide-react";
import { RequiredScopesCard } from "./RequiredScopesCard";

interface GitHubScope {
  name: string;
  description: string;
}

const GITHUB_SCOPES: GitHubScope[] = [
  { name: "repo", description: "Access to repositories" },
  { name: "user:email", description: "Access to user email addresses" },
  { name: "read:user", description: "Access to user profile information" },
];

const GITHUB_TOKEN_URL =
  "https://github.com/settings/tokens/new?scopes=repo,user:email,read:user&description=Oxy%20Integration";

const openSecureWindow = (url: string): void => {
  const newWindow = window.open(url, "_blank", "noopener,noreferrer");
  if (newWindow) newWindow.opener = null;
};

interface GitHubTokenInputProps {
  token: string;
  onTokenChange: (token: string) => void;
  disabled?: boolean;
  error?: string;
}

export const GitHubTokenInput: React.FC<GitHubTokenInputProps> = ({
  token,
  onTokenChange,
  disabled = false,
  error,
}) => {
  const [showToken, setShowToken] = useState(false);

  const handleOpenTokenPage = () => {
    openSecureWindow(GITHUB_TOKEN_URL);
  };

  return (
    <div className="space-y-2">
      <Label htmlFor="githubToken">Access Token</Label>
      <RequiredScopesCard
        scopes={GITHUB_SCOPES}
        onOpenTokenPage={handleOpenTokenPage}
      />
      <div className="relative">
        <Input
          id="githubToken"
          type={showToken ? "text" : "password"}
          placeholder="ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
          value={token}
          onChange={(e) => onTokenChange(e.target.value)}
          disabled={disabled}
          className="pr-10"
        />
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="absolute right-0 top-0 h-full px-3 py-2 hover:bg-transparent"
          onClick={() => setShowToken(!showToken)}
          disabled={disabled}
        >
          {showToken ? (
            <EyeOff className="h-4 w-4" />
          ) : (
            <Eye className="h-4 w-4" />
          )}
        </Button>
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}
    </div>
  );
};
