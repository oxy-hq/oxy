import { isAxiosError } from "axios";
import { ExternalLink, ShieldCheck } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { cn } from "@/libs/shadcn/utils";
import type { OxyAccessGrant } from "@/types/apps";
import { openWorkspaceHome } from "../../openWorkspaceHome";

/** One org with the workspaces it granted Oxy access to. */
export interface OrgGroup {
  orgId: string;
  orgName: string;
  orgSlug: string;
  projects: OxyAccessGrant[];
}

/** Fold the flat grant list into per-org groups, each sorted by workspace. */
export function groupByOrg(grants: OxyAccessGrant[]): OrgGroup[] {
  const byId = new Map<string, OrgGroup>();
  for (const g of grants) {
    const group = byId.get(g.org_id) ?? {
      orgId: g.org_id,
      orgName: g.org_name,
      orgSlug: g.org_slug,
      projects: []
    };
    group.projects.push(g);
    byId.set(g.org_id, group);
  }
  return [...byId.values()].sort((a, b) => a.orgName.localeCompare(b.orgName));
}

export function formatGrantedAt(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString();
}

/** Resizable list/detail skeleton shared by both panes (matches the Apps tab). */
export const MasterDetail = ({
  list,
  detail
}: {
  list: React.ReactNode;
  detail: React.ReactNode;
}) => (
  <ResizablePanelGroup direction='horizontal' className='min-h-0 flex-1'>
    <ResizablePanel defaultSize={32} minSize={20} maxSize={50}>
      {list}
    </ResizablePanel>
    <ResizableHandle withHandle />
    <ResizablePanel defaultSize={68} minSize={40}>
      {detail}
    </ResizablePanel>
  </ResizablePanelGroup>
);

export const PaneSearch = ({
  value,
  onChange,
  placeholder
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}) => (
  <div className='border-border border-b p-2'>
    <Input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className='h-8'
    />
  </div>
);

/** A selectable row in either list pane: bold title + muted subtitle. */
export const ListRow = ({
  active,
  onClick,
  title,
  subtitle,
  trailing
}: {
  active: boolean;
  onClick: () => void;
  title: string;
  subtitle: string;
  trailing?: React.ReactNode;
}) => (
  <button
    type='button'
    onClick={onClick}
    className={cn(
      "flex w-full items-center gap-2 border-border/50 border-b px-3 py-2 text-left transition-colors",
      active ? "bg-primary/10" : "hover:bg-muted/50"
    )}
  >
    <span className='min-w-0 flex-1'>
      <span className='block truncate font-medium text-sm'>{title}</span>
      <span className='block truncate text-muted-foreground text-xs'>{subtitle}</span>
    </span>
    {trailing}
  </button>
);

/** "Open /home" — switches into the workspace via the normal dispatcher. */
export const OpenHomeButton = ({ grant }: { grant: OxyAccessGrant }) => {
  const navigate = useNavigate();
  return (
    <Button
      size='sm'
      variant='outline'
      className='gap-1.5'
      onClick={() => openWorkspaceHome(grant, navigate)}
    >
      Open /home
      <ExternalLink className='size-3.5' />
    </Button>
  );
};

export const GrantsLoading = () => (
  <div className='flex h-full items-center justify-center'>
    <Spinner className='size-5' />
  </div>
);

export const EmptyHint = ({ title, body }: { title: string; body: string }) => (
  <div className='flex h-full flex-col items-center justify-center gap-3 bg-muted/20 px-6 text-center'>
    <div className='flex size-12 items-center justify-center rounded-full border bg-background shadow-sm'>
      <ShieldCheck className='size-5 text-muted-foreground' />
    </div>
    <div>
      <p className='font-medium text-foreground text-sm'>{title}</p>
      <p className='mt-1 max-w-sm text-muted-foreground text-sm'>{body}</p>
    </div>
  </div>
);

/** 403-aware error block matching the Apps tab's allow-list message. */
export const GrantsError = ({ error }: { error: unknown }) => (
  <div className='mx-auto max-w-2xl p-6'>
    <div className='rounded-lg border border-destructive/30 bg-destructive/5 p-6 text-center'>
      {isAxiosError(error) && error.response?.status === 403 ? (
        <>
          <p className='font-medium text-destructive text-sm'>
            Your account isn't on the customer-apps allow list.
          </p>
          <p className='mt-2 text-muted-foreground text-xs'>
            Add your email to the oxy backend's{" "}
            <code className='rounded bg-muted px-1 py-0.5 font-mono'>OXY_GLOBAL_ADMINS</code> and
            refresh.
          </p>
        </>
      ) : (
        <p className='text-destructive text-sm'>Failed to load Oxy-access grants.</p>
      )}
    </div>
  </div>
);
