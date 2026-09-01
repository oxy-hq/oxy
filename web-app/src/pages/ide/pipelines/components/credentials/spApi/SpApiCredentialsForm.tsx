import type React from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import type { useSpApiCredentials } from "./useSpApiCredentials";

interface SpApiCredentialsFormProps {
  credentials: ReturnType<typeof useSpApiCredentials>;
  onClearError: () => void;
}

const SpApiCredentialsForm: React.FC<SpApiCredentialsFormProps> = ({
  credentials,
  onClearError
}) => (
  <div className='grid gap-3 rounded-md border border-border p-3'>
    <p className='font-medium text-sm'>Amazon Selling Partner credentials</p>

    <div className='grid gap-2'>
      <Label htmlFor='sp-api-client-id'>LWA Client ID</Label>
      <Input
        id='sp-api-client-id'
        value={credentials.clientId}
        onChange={(e) => {
          credentials.setClientId(e.target.value);
          onClearError();
        }}
        placeholder='amzn1.application-oa2-client...'
      />
      <p className='text-muted-foreground text-xs'>
        Identifies the application, not the seller — it travels in every token request, so it is
        stored in the pipeline file rather than the secret manager.
      </p>
    </div>

    <div className='grid gap-2'>
      <Label htmlFor='sp-api-client-secret'>LWA client secret</Label>
      <Input
        id='sp-api-client-secret'
        type='password'
        value={credentials.clientSecret}
        onChange={(e) => credentials.setClientSecret(e.target.value)}
        placeholder='Stored as a secret — leave blank to reuse an existing one'
      />
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='sp-api-client-secret-name'>Client secret name</Label>
      <Input
        id='sp-api-client-secret-name'
        value={credentials.clientSecretName}
        onChange={(e) => {
          credentials.setClientSecretName(e.target.value);
          onClearError();
        }}
        placeholder='SP_API_CLIENT_SECRET'
      />
    </div>

    <div className='grid gap-2'>
      <Label htmlFor='sp-api-refresh-token'>Refresh token</Label>
      <Input
        id='sp-api-refresh-token'
        type='password'
        value={credentials.refreshToken}
        onChange={(e) => credentials.setRefreshToken(e.target.value)}
        placeholder='Atzr|... — leave blank to reuse an existing secret'
      />
      <p className='text-muted-foreground text-xs'>
        Issued when the app is authorized in Seller Central. This is the credential that grants
        access to this seller&apos;s data — rotate it there to revoke access.
      </p>
    </div>
    <div className='grid gap-2'>
      <Label htmlFor='sp-api-refresh-token-name'>Refresh token name</Label>
      <Input
        id='sp-api-refresh-token-name'
        value={credentials.refreshTokenName}
        onChange={(e) => {
          credentials.setRefreshTokenName(e.target.value);
          onClearError();
        }}
        placeholder='SP_API_REFRESH_TOKEN'
      />
      <p className='text-muted-foreground text-xs'>
        The pipeline references these names; the executor resolves them from the secret manager at
        run time. Reuse an existing secret by entering its name and leaving the value blank.
      </p>
    </div>

    <div className='grid gap-2'>
      <Label htmlFor='sp-api-marketplace'>Marketplace</Label>
      <Select
        value={credentials.marketplaceId}
        onValueChange={(v) => {
          credentials.setMarketplaceId(v);
          onClearError();
        }}
        disabled={!credentials.marketplaces.data?.length}
      >
        <SelectTrigger id='sp-api-marketplace'>
          <SelectValue
            placeholder={
              credentials.marketplaces.isError
                ? "Could not load marketplaces"
                : credentials.marketplaces.isPending
                  ? "Loading…"
                  : "Select a marketplace"
            }
          />
        </SelectTrigger>
        <SelectContent>
          {(credentials.marketplaces.data ?? []).map((m) => (
            <SelectItem key={m.id} value={m.id} title={m.id}>
              {m.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className='text-muted-foreground text-xs'>
        North America only — the connector pins the NA endpoint, so an EU or Far East marketplace
        would be refused with a 403 that reads like a bad credential. The list comes from the
        server, so it always matches what the connector will accept.
      </p>
    </div>

    <div className='grid gap-2'>
      <Label htmlFor='sp-api-default-start'>Start date</Label>
      <Input
        id='sp-api-default-start'
        type='date'
        value={credentials.defaultStart}
        onChange={(e) => {
          credentials.setDefaultStart(e.target.value);
          onClearError();
        }}
      />
      <p className='text-muted-foreground text-xs'>
        Where the first run begins, and the entire backfill policy — this connector only pulls
        forward, so anything earlier stays missing until someone resets the cursor. Reaching too far
        back is the riskier direction: the whole span is requested as a <em>single</em> report, and
        a window too large for Amazon to build in time fails the same way on every later run rather
        than catching up. Defaults to the first of last month, which keeps the span between one and
        two months.
      </p>
    </div>
  </div>
);

export default SpApiCredentialsForm;
