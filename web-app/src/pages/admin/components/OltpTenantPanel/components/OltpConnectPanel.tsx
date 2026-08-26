import { Copy, Eye, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import type { useOltpCredentials } from "@/hooks/api/oltp/useAdminOltp";
import { AdminSectionLabel } from "@/pages/admin/components/AdminSectionLabel";
import type { OltpConnectionInfo } from "@/services/api/oltp";

/**
 * Something to paste into `psql`.
 *
 * Analyst first and on its own line: "let me look at the database" almost
 * always means reading it, and that credential needs no warning. Writer
 * credentials sit below, marked, because each is a live write credential to one
 * app's schema.
 *
 * The two-line paragraph explaining what the analyst is has gone — it told an
 * operator something they learn once, every time they open the page. What is
 * left is the label on the button and the WRITABLE badge on the result, which
 * are the parts that matter at the moment of the click.
 */
export const OltpConnectPanel = ({
  data,
  credentials
}: {
  data: OltpConnectionInfo;
  credentials: ReturnType<typeof useOltpCredentials>;
}) => {
  const copy = (dsn: string) => {
    void navigator.clipboard.writeText(dsn);
    toast.success("Connection string copied");
  };

  return (
    <div className='flex flex-col gap-2' data-testid='admin-org-oltp-connect'>
      <AdminSectionLabel trailing='disclosure is logged'>Connect</AdminSectionLabel>

      {/* `w-fit`: stretched to the column it read as an input field rather
          than an action, and gave the safest credential on the page the most
          visual weight. */}
      <Button
        size='sm'
        variant='outline'
        className='h-7 w-fit px-2'
        disabled={credentials.isPending}
        onClick={() => credentials.mutate("analyst")}
        data-testid='admin-org-oltp-reveal-analyst'
      >
        <Eye className='size-3' />
        Analyst DSN — read-only
      </Button>

      {data.schemas.length > 0 && (
        <div className='flex flex-wrap items-center gap-1'>
          {/* Labelled, because these repeat the schema names shown in the strip
              across the page and only the icon said they mean something else
              here: one is "what exists", this is "a live write credential". */}
          <span className='mr-0.5 text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
            write
          </span>
          {/* Plain buttons: `<Button>` carries `.t-button`, whose unlayered
              font-size wins over a size utility on the element, so these would
              render at 14px beside the chips they mirror. */}
          {data.schemas.map((s) => (
            <button
              key={s.role}
              type='button'
              disabled={credentials.isPending}
              onClick={() => credentials.mutate(`${s.kind}:${s.writer_name}`)}
              className='inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 font-mono text-muted-foreground text-xs outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50 disabled:opacity-50'
              data-testid={`admin-org-oltp-reveal-${s.schema}`}
            >
              <ShieldAlert className='size-3 shrink-0' />
              {s.schema}
            </button>
          ))}
        </div>
      )}

      {credentials.data && (
        <div className='flex items-center gap-1.5' data-testid='admin-org-oltp-dsn'>
          <code className='min-w-0 flex-1 truncate rounded bg-muted px-1.5 py-1 font-mono text-xs'>
            {credentials.data.dsn}
          </code>
          <Badge variant={credentials.data.writable ? "destructive" : "secondary"}>
            {credentials.data.writable ? "WRITABLE" : "read-only"}
          </Badge>
          <Button
            size='sm'
            variant='ghost'
            className='size-6 shrink-0 p-0'
            onClick={() => copy(credentials.data.dsn)}
            aria-label='Copy connection string'
          >
            <Copy className='size-3' />
          </Button>
        </div>
      )}
    </div>
  );
};
