import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Button } from "@/components/ui/shadcn/button";
import { ExternalLink } from "lucide-react";

interface Scope {
  name: string;
  description: string;
}

interface RequiredScopesCardProps {
  scopes: Scope[];
  onOpenTokenPage: () => void;
}

/**
 * Card component that displays required GitHub token scopes and provides
 * a button to create a new token with the correct permissions
 */
export const RequiredScopesCard = ({
  scopes,
  onOpenTokenPage,
}: RequiredScopesCardProps) => {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ExternalLink className="h-5 w-5" />
          Create GitHub Personal Access Token
        </CardTitle>
        <CardDescription>
          You'll need to create a Personal Access Token with the following
          permissions:
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-3 mb-4">
          {scopes.map((scope, index) => (
            <div key={index} className="flex items-center space-x-3">
              <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
              <div>
                <span className="font-mono text-sm bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded">
                  {scope.name}
                </span>
                <span className="text-sm text-gray-600 dark:text-gray-400 ml-2">
                  - {scope.description}
                </span>
              </div>
            </div>
          ))}
        </div>

        <Button onClick={onOpenTokenPage} variant="outline" className="w-full">
          <ExternalLink className="h-4 w-4 mr-2" />
          Create Token on GitHub
        </Button>

        <div className="mt-4 p-3 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg">
          <p className="text-sm text-yellow-800 dark:text-yellow-200">
            <strong>Important:</strong> Copy your token immediately after
            creating it. GitHub will only show it once for security reasons.
          </p>
        </div>
      </CardContent>
    </Card>
  );
};
