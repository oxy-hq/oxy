import { useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import { Eye, EyeOff, Check, X } from "lucide-react";
import type { ValidationStatus } from "../types/github";

interface TokenInputCardProps {
  token: string;
  onTokenChange: (token: string) => void;
  onValidate: () => void;
  isValidating: boolean;
  validationStatus: ValidationStatus;
}

/**
 * Card component for GitHub token input and validation
 *
 * Features:
 * - Token input with show/hide functionality
 * - Real-time validation status display
 * - Validation trigger button
 */
export const TokenInputCard = ({
  token,
  onTokenChange,
  onValidate,
  isValidating,
  validationStatus,
}: TokenInputCardProps) => {
  const [showToken, setShowToken] = useState(false);

  const getValidationIcon = () => {
    if (validationStatus === true)
      return <Check className="h-4 w-4 text-green-500" />;
    if (validationStatus === false)
      return <X className="h-4 w-4 text-red-500" />;
    return null;
  };

  const getButtonText = () => {
    if (isValidating) return "Validating...";
    if (validationStatus === true) return "Token Validated ✅";
    return "Validate Token";
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Enter Your GitHub Token</CardTitle>
        <CardDescription>
          Paste the Personal Access Token you just created
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="token">GitHub Personal Access Token</Label>
            <div className="relative">
              <Input
                id="token"
                type={showToken ? "text" : "password"}
                value={token}
                onChange={(e) => onTokenChange(e.target.value)}
                placeholder="ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
                className="pr-20"
                disabled={isValidating || validationStatus === true}
              />
              <div className="absolute inset-y-0 right-0 flex items-center space-x-1 pr-3">
                {getValidationIcon()}
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setShowToken(!showToken)}
                  className="h-7 w-7 p-0"
                >
                  {showToken ? (
                    <EyeOff className="h-4 w-4" />
                  ) : (
                    <Eye className="h-4 w-4" />
                  )}
                </Button>
              </div>
            </div>
          </div>

          <Button
            onClick={onValidate}
            disabled={
              !token.trim() || isValidating || validationStatus === true
            }
            className="w-full continue-button"
          >
            {getButtonText()}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
};
