import type React from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { useToastCredentials } from "./useToastCredentials";

interface ToastCredentialsFormProps {
  credentials: ReturnType<typeof useToastCredentials>;
  onClearError: () => void;
}

const ToastCredentialsForm: React.FC<ToastCredentialsFormProps> = ({
  credentials,
  onClearError
}) => (
  <div className='grid gap-3 rounded-md border border-border p-3'>
    <p className='font-medium text-sm'>Toast credentials</p>
    <div className='grid gap-2'>
      <Label htmlFor='toast-client-id'>Client ID</Label>
      <Input
        id='toast-client-id'
        value={credentials.clientId}
        onChange={(e) => {
          credentials.setClientId(e.target.value);
          onClearError();
        }}
        placeholder='Toast OAuth2 client id'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='toast-client-secret'>Client secret</Label>
      <Input
        id='toast-client-secret'
        type='password'
        value={credentials.clientSecret}
        onChange={(e) => credentials.setClientSecret(e.target.value)}
        placeholder='Stored as a secret — leave blank to reuse an existing one'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='toast-secret-name'>Secret name</Label>
      <Input
        id='toast-secret-name'
        value={credentials.secretName}
        onChange={(e) => {
          credentials.setSecretName(e.target.value);
          onClearError();
        }}
        placeholder='TOAST_CLIENT_SECRET'
      />
      <p className='text-muted-foreground text-xs'>
        The pipeline references this name; the executor resolves it from the secret manager at run
        time. Reuse an existing secret by entering its name and leaving the value blank.
      </p>
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='toast-guids'>Restaurant GUID(s)</Label>
      <Textarea
        id='toast-guids'
        value={credentials.guids}
        onChange={(e) => {
          credentials.setGuids(e.target.value);
          onClearError();
        }}
        placeholder='One per line (or comma-separated)'
        rows={2}
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='toast-base-url'>Base URL</Label>
      <Input
        id='toast-base-url'
        value={credentials.baseUrl}
        onChange={(e) => credentials.setBaseUrl(e.target.value)}
        placeholder='Optional — sandbox override (defaults to Toast prod)'
      />
    </div>
  </div>
);

export default ToastCredentialsForm;
