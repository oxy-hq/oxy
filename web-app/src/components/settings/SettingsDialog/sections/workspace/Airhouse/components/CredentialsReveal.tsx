import { Eye, RefreshCw, X } from "lucide-react";
import type React from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/shadcn/card";
import { CopyableField } from "./CopyableField";

interface CredentialsRevealProps {
  passwordAlreadyRevealed: boolean;
  shownPassword: string | null;
  /// True while a /credentials reveal is in flight.
  isRevealing: boolean;
  /// True while a /rotate-password call is in flight.
  isRotating: boolean;
  revealError: unknown;
  rotateError: unknown;
  onReveal: () => void;
  onRotate: () => void;
  onDismissPassword: () => void;
}

export const CredentialsReveal: React.FC<CredentialsRevealProps> = ({
  passwordAlreadyRevealed,
  shownPassword,
  isRevealing,
  isRotating,
  revealError,
  rotateError,
  onReveal,
  onRotate,
  onDismissPassword
}) => {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Password</CardTitle>
        <p className='text-muted-foreground text-sm'>
          Your password is stored encrypted and can be shown again at any time. Rotate it if you
          suspect it has leaked or if a teammate needs to be locked out.
        </p>
      </CardHeader>
      <CardContent className='space-y-3'>
        {shownPassword ? (
          <>
            <Alert>
              <AlertTitle>{passwordAlreadyRevealed ? "Password" : "New password"}</AlertTitle>
              <AlertDescription>
                Copy this into your client now. You can come back and reveal it again, but storing
                it in a password manager is recommended.
              </AlertDescription>
            </Alert>
            <CopyableField label='Password' value={shownPassword} />
          </>
        ) : null}
        <div className='flex flex-wrap gap-2'>
          <Button onClick={onReveal} disabled={isRevealing || isRotating}>
            <Eye className='h-4 w-4' />
            {isRevealing ? "Loading…" : shownPassword ? "Reveal again" : "Show password"}
          </Button>
          <Button variant='outline' onClick={onRotate} disabled={isRevealing || isRotating}>
            <RefreshCw className='h-4 w-4' />
            {isRotating ? "Rotating…" : "Rotate password"}
          </Button>
          {shownPassword ? (
            <Button variant='outline' onClick={onDismissPassword}>
              <X className='h-4 w-4' />
              Hide
            </Button>
          ) : null}
        </div>
        {revealError ? (
          <Alert variant='destructive'>
            <AlertDescription>
              Failed to fetch your password. Please try again or contact an admin.
            </AlertDescription>
          </Alert>
        ) : null}
        {rotateError ? (
          <Alert variant='destructive'>
            <AlertDescription>
              Password rotation failed. The previous password is still valid; try again in a moment
              or contact an admin.
            </AlertDescription>
          </Alert>
        ) : null}
      </CardContent>
    </Card>
  );
};
