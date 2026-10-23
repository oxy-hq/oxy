import { ExternalLink, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import type { CustomDomain } from "@/types/apps";
import { relativeTime } from "../Activity/relativeTime";
import { DnsTable } from "./DnsTable";
import { dnsRecords } from "./dnsRecords";

/**
 * One domain. Pending: the two DNS records to create, a Verify button, and why the
 * last check failed. Verified: the live URL and Remove. A pending domain can be
 * removed too, so a typo does not stay in the list.
 */
export const DomainRow = ({
  domain,
  cnameTarget,
  verifying,
  onVerify,
  onRemove
}: {
  domain: CustomDomain;
  cnameTarget: string | null;
  verifying: boolean;
  onVerify: () => void;
  onRemove: () => void;
}) => {
  const verified = domain.status === "verified";
  const tone = verified ? ADMIN_TONE.ok : ADMIN_TONE.warn;
  return (
    <li
      className='flex flex-col gap-2 px-3 py-2 text-xs'
      data-testid={`admin-app-domain-row-${domain.id}`}
    >
      <div className='flex items-center gap-2'>
        <span className='min-w-0 flex-1 truncate font-medium font-mono' title={domain.domain}>
          {domain.domain}
        </span>
        <Badge
          variant='outline'
          className={cn(tone.bg, tone.text, "ring-1 ring-inset", tone.ring)}
          data-testid={`admin-app-domain-status-${domain.id}`}
        >
          {verified ? "Verified" : "Pending"}
        </Badge>
        {!verified && (
          <Button
            size='sm'
            variant='outline'
            className='h-6 px-2 text-[11px]'
            disabled={verifying}
            onClick={onVerify}
            data-testid={`admin-app-domain-verify-${domain.id}`}
          >
            {verifying ? "Checking…" : "Verify"}
          </Button>
        )}
        <Button
          variant='ghost'
          size='sm'
          className='h-6 gap-1 px-1.5 text-[11px] text-muted-foreground'
          onClick={onRemove}
          data-testid={`admin-app-domain-remove-${domain.id}`}
        >
          <Trash2 className='size-3' />
          Remove
        </Button>
      </div>

      {verified ? (
        <a
          href={domain.url}
          target='_blank'
          rel='noopener noreferrer'
          className='flex w-fit items-center gap-1 font-mono text-primary hover:underline'
          data-testid={`admin-app-domain-link-${domain.id}`}
        >
          {domain.url}
          <ExternalLink className='size-3' />
        </a>
      ) : (
        <>
          <p className='text-muted-foreground'>
            Create these records at your DNS host, then verify.
          </p>
          <DnsTable domainId={domain.id} records={dnsRecords(domain, cnameTarget)} />
          {domain.last_check_error && (
            <p
              className={cn(ADMIN_TONE.danger.text)}
              data-testid={`admin-app-domain-error-${domain.id}`}
            >
              {domain.last_check_error}
              {domain.last_checked_at ? ` · checked ${relativeTime(domain.last_checked_at)}` : ""}
            </p>
          )}
        </>
      )}
    </li>
  );
};
