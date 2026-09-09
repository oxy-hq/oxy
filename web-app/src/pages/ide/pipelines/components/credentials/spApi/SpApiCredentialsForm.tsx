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
import type { SpApiPartnerType } from "../../../scaffold";
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
      <Label htmlFor='sp-api-partner-type'>Amazon account</Label>
      <Select
        value={credentials.partnerType}
        onValueChange={(v) => {
          credentials.setPartnerType(v as SpApiPartnerType);
          onClearError();
        }}
      >
        <SelectTrigger id='sp-api-partner-type'>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value='seller'>Seller Central</SelectItem>
          <SelectItem value='vendor'>Vendor Central</SelectItem>
        </SelectContent>
      </Select>
      <p className='text-muted-foreground text-xs'>
        Separate Amazon accounts with separate authorizations, not two views of one — so the
        credentials below belong to whichever you pick, and each needs its own pipeline. This
        decides which reports the pipeline can pull <em>at all</em>: choosing the wrong one does not
        fetch the wrong data, it fetches nothing, and every report is refused for want of the role.
        Switching this re-points the secret names below and clears any credential you have pasted,
        since both belong to the account — so a vendor pipeline cannot end up holding the
        seller&apos;s token under a vendor name.
      </p>
    </div>

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
        Issued when the app is authorized in the account&apos;s own console — Seller Central or
        Vendor Central, whichever you picked above. This is the credential that grants access to
        that account&apos;s data; rotate it there to revoke access.
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
        forward, so anything earlier stays missing until someone resets the cursor. A long span no
        longer stalls the way it once did: it is split into report-sized windows Amazon will finish,
        and an interrupted run resumes at the one it reached. It still costs one report job per
        window on that first run, against a per-account budget that refills about once a minute, so
        reaching far back is slow rather than free. Defaults to the first of last month, which keeps
        the span between one and two months.
      </p>
    </div>
  </div>
);

export default SpApiCredentialsForm;
