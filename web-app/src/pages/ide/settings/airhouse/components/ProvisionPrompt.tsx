import { isAxiosError } from "axios";
import type React from "react";
import { useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/shadcn/alert";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AirhouseLogo } from "../../components/AirhouseLogo";

// Mirrors the backend validate_tenant_name regex:
// starts with [a-z], body [a-z0-9_-], 1-63 chars total.
const TENANT_NAME_RE = /^[a-z][a-z0-9_-]{0,61}[a-z0-9]$|^[a-z]$/;

function validateTenantName(name: string): string | null {
  if (!name) return "Tenant name is required.";
  if (!TENANT_NAME_RE.test(name))
    return "Must be 1-63 characters, start with a lowercase letter, and contain only lowercase letters, digits, hyphens, or underscores.";
  return null;
}

function errorMessage(err: unknown): string {
  if (isAxiosError(err) && err.response?.status === 422) {
    return "Invalid tenant name — it must start with a lowercase letter and contain only lowercase letters, digits, hyphens, or underscores (1-63 chars).";
  }
  return "Provisioning failed. The Airhouse server may be unreachable — try again in a moment or contact an administrator.";
}

interface ProvisionPromptProps {
  onProvision: (tenantName: string) => void;
  isPending: boolean;
  error: unknown;
}

export const ProvisionPrompt: React.FC<ProvisionPromptProps> = ({
  onProvision,
  isPending,
  error
}) => {
  const [tenantName, setTenantName] = useState("");
  const [touched, setTouched] = useState(false);

  const validationError = validateTenantName(tenantName);

  const handleSubmit = () => {
    setTouched(true);
    if (validationError) return;
    onProvision(tenantName);
  };

  return (
    <Card>
      <CardHeader>
        <AirhouseLogo className='h-6' />
        <CardTitle>Set up your Airhouse connection</CardTitle>
        <p className='text-muted-foreground text-sm'>
          You don't have an Airhouse user in this workspace yet. Choose a tenant name and click
          below to provision one. This creates an Airhouse tenant and user, generates a password,
          and prepares your connection details. You can copy the password from this page afterwards.
        </p>
      </CardHeader>
      <CardContent className='space-y-4'>
        <div className='space-y-2'>
          <Label htmlFor='tenant-name'>Tenant name</Label>
          <Input
            id='tenant-name'
            placeholder='my-workspace'
            value={tenantName}
            onChange={(e) => setTenantName(e.target.value)}
            onBlur={() => setTouched(true)}
            disabled={isPending}
          />
          {touched && validationError ? (
            <p className='text-destructive text-sm'>{validationError}</p>
          ) : (
            <p className='text-muted-foreground text-xs'>
              Lowercase letters, digits, hyphens, and underscores only. Must start with a letter
              (1-63 chars).
            </p>
          )}
        </div>
        <Button onClick={handleSubmit} disabled={isPending || (touched && !!validationError)}>
          {isPending ? "Provisioning…" : "Provision Airhouse access"}
        </Button>
        {error ? (
          <Alert variant='destructive'>
            <AlertDescription>{errorMessage(error)}</AlertDescription>
          </Alert>
        ) : null}
      </CardContent>
    </Card>
  );
};
