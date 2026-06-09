import { Check } from "lucide-react";
import type React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { apiBaseURL } from "@/services/env";
import type { useQuickBooksCredentials } from "./useQuickBooksCredentials";

interface QuickBooksCredentialsFormProps {
  credentials: ReturnType<typeof useQuickBooksCredentials>;
  onClearError: () => void;
  onConnect: () => void;
}

const QuickBooksCredentialsForm: React.FC<QuickBooksCredentialsFormProps> = ({
  credentials,
  onClearError,
  onConnect
}) => (
  <div className='grid gap-3 rounded-md border border-border p-3'>
    <p className='font-medium text-sm'>QuickBooks Online credentials</p>
    <p className='text-muted-foreground text-xs'>
      Enter your Intuit app's client id + secret, then Connect — the consent flow captures the
      company (realm) id and refresh token for you.
    </p>
    <div className='grid gap-2'>
      <Label htmlFor='qb-client-id'>
        Client ID <span className='text-destructive'>*</span>
      </Label>
      <Input
        id='qb-client-id'
        value={credentials.clientId}
        onChange={(e) => {
          credentials.setClientId(e.target.value);
          onClearError();
        }}
        placeholder='Intuit OAuth2 client id'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='qb-client-secret'>
        Client secret <span className='text-destructive'>*</span>
      </Label>
      <Input
        id='qb-client-secret'
        type='password'
        value={credentials.clientSecret}
        onChange={(e) => credentials.setClientSecret(e.target.value)}
        placeholder='Stored securely when you Connect'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='qb-client-secret-name'>
        Client secret name <span className='text-destructive'>*</span>
      </Label>
      <Input
        id='qb-client-secret-name'
        value={credentials.clientSecretName}
        onChange={(e) => {
          credentials.setClientSecretName(e.target.value);
          onClearError();
        }}
        placeholder='QB_CLIENT_SECRET'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='qb-refresh-token-name'>
        Refresh token name <span className='text-destructive'>*</span>
      </Label>
      <Input
        id='qb-refresh-token-name'
        value={credentials.refreshTokenName}
        onChange={(e) => {
          credentials.setRefreshTokenName(e.target.value);
          onClearError();
        }}
        placeholder='QB_REFRESH_TOKEN'
      />
      <p className='text-muted-foreground text-xs'>
        Connect stores the captured refresh token under this name and the pipeline reads it at run
        time; Intuit rotates it on every use and the new value is written back here automatically.
      </p>
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='qb-base-url'>
        Base URL <span className='text-muted-foreground text-xs'>(optional)</span>
      </Label>
      <Input
        id='qb-base-url'
        value={credentials.baseUrl}
        onChange={(e) => credentials.setBaseUrl(e.target.value)}
        placeholder='Sandbox override (defaults to QuickBooks prod)'
      />
    </div>
    <div className='grid gap-2 border-border border-t pt-3'>
      <Button
        type='button'
        variant='outline'
        onClick={onConnect}
        disabled={credentials.connecting}
        data-testid='qb-connect-button'
      >
        {credentials.connecting
          ? "Connecting…"
          : credentials.connected
            ? "Reconnect QuickBooks"
            : "Connect with QuickBooks"}
      </Button>
      {credentials.connected ? (
        <p className='flex items-center gap-1 text-muted-foreground text-xs'>
          <Check className='h-3 w-3 text-primary' />
          Connected{credentials.realmId ? ` — company ${credentials.realmId}` : ""}. Refresh token
          stored.
        </p>
      ) : (
        <p className='text-muted-foreground text-xs'>
          Authorize with Intuit to capture the realm id + refresh token automatically. Register this
          Redirect URI in your Intuit app:{" "}
          <code className='break-all'>{`${apiBaseURL}/quickbooks/oauth/callback`}</code>
        </p>
      )}
    </div>
  </div>
);

export default QuickBooksCredentialsForm;
