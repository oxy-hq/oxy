import { Copy } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { useCopyAppSecret } from "@/hooks/api/customApps/useAppSecrets";
import { errMessage } from "@/hooks/api/errMessage";
import { PRODUCTION } from "@/types/appEnvironments";
import type { AppSecretEntry } from "@/types/apps";
import { STAGING } from "./environmentLabel";

/** The environments a value may be copied from into `environment`: only
 *  downhill, as the server allows — production into staging, and staging or
 *  production into a sandbox. Nothing into production. */
export function copySources(environment: string): string[] {
  if (environment === PRODUCTION) return [];
  if (environment === STAGING) return [PRODUCTION];
  return [STAGING, PRODUCTION];
}

/**
 * The copies worth offering on this row.
 *
 * **A source only when it holds the key**, since the copy reads that
 * environment's own row and a copy of nothing can only 404. Production does when
 * `productionHolds` (read from production's own view; unknown counts as no).
 * Staging does when the server says so with `inherits_staging` on an empty
 * sandbox row.
 *
 * **Only on a row with nothing stored.** A copy is an upsert, the store keeps
 * no history, and a staging value is often deliberately different — a sandbox
 * company's refresh token, say. Overwriting one with production's would hand
 * staging code a production credential, and for a token that rotates on use
 * (QuickBooks voids the old one on every issue) staging's next rotation breaks
 * production. Replacing a value is delete, then copy: two deliberate steps.
 *
 */
export function offeredSources(
  environment: string,
  entry: AppSecretEntry,
  productionHolds = false
): string[] {
  if (entry.is_set) return [];
  return copySources(environment).filter((from) =>
    from === STAGING ? !!entry.inherits_staging : productionHolds
  );
}

/**
 * "Copy from production / staging" on an empty non-production row
 * (`offeredSources`). The copy happens
 * on the server (`copy_from` on the set route), so the value is never revealed
 * to the browser to be pasted back — which is what staff did before, through
 * Reveal, leaving a plaintext production credential in a clipboard.
 *
 * A declared key that is not `shared` also says how to stop needing a copy:
 * marking it shared makes this environment read production's value directly.
 */
export const CopyFrom = ({
  appId,
  environment,
  entry,
  productionHolds
}: {
  appId: string;
  environment: string;
  entry: AppSecretEntry;
  /** Production stores this key; `undefined` while that is unknown. */
  productionHolds?: boolean;
}) => {
  const copy = useCopyAppSecret(appId, environment);
  const sources = offeredSources(environment, entry, productionHolds);
  if (sources.length === 0) return null;

  const run = (from: string) =>
    copy.mutate(
      { key: entry.key, from },
      {
        onSuccess: () => toast.success(`Copied ${entry.key} from ${from}.`),
        onError: (e) => toast.error(errMessage(e, `Couldn't copy ${entry.key} from ${from}.`))
      }
    );

  return (
    <div className='mt-1 flex flex-wrap items-center gap-1 pl-5'>
      {sources.map((from) => (
        <Button
          key={from}
          type='button'
          size='sm'
          variant='ghost'
          className='h-5 gap-1 px-1.5 text-[11px] text-muted-foreground'
          disabled={copy.isPending}
          onClick={() => run(from)}
        >
          <Copy className='size-3' />
          Copy from {from}
        </Button>
      ))}
      {entry.declared && !entry.shared && (
        <span className='text-[11px] text-muted-foreground'>
          or mark it <code>&quot;shared&quot;: true</code> in <code>oxy-app.json</code> to read
          production&rsquo;s value.
        </span>
      )}
    </div>
  );
};
