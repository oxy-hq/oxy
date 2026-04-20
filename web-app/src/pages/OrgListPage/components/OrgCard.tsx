import { Boxes, Users } from "lucide-react";
import { formatRelativeDate } from "@/libs/utils/date";
import type { Organization } from "@/types/organization";

type OrgCardProps = {
  org: Organization;
  index: number;
  onSelect: () => void;
};

export default function OrgCard({ org, index, onSelect }: OrgCardProps) {
  const workspaceCount = org.workspace_count ?? 0;
  const memberCount = org.member_count ?? 0;
  const createdAt = formatRelativeDate(org.created_at);

  return (
    <li
      className='fade-in slide-in-from-bottom-2 h-full animate-in fill-mode-both duration-300'
      style={{ animationDelay: `${index * 60}ms` }}
    >
      <button
        type='button'
        onClick={onSelect}
        className='group relative flex h-full w-full flex-col overflow-hidden rounded-xl border border-border bg-card text-left transition-all hover:border-border/60 hover:shadow-sm'
      >
        <div className='flex flex-1 flex-col gap-2.5 p-5'>
          <div className='flex items-start justify-between gap-3'>
            <span className='font-semibold text-[15px] text-foreground leading-snug tracking-tight'>
              {org.name}
            </span>
            <span className='shrink-0 rounded-full bg-primary/10 px-2.5 py-1 font-medium text-[11px] text-primary capitalize'>
              {org.role}
            </span>
          </div>
          <span className='font-mono text-[11px] text-muted-foreground/50'>{org.slug}</span>
          <div className='mt-1 flex items-center gap-3 text-[11px] text-muted-foreground/70'>
            <span className='flex items-center gap-1'>
              <Boxes className='size-3' />
              {workspaceCount} {workspaceCount === 1 ? "workspace" : "workspaces"}
            </span>
            <span className='flex items-center gap-1'>
              <Users className='size-3' />
              {memberCount} {memberCount === 1 ? "member" : "members"}
            </span>
          </div>
        </div>
        {createdAt && (
          <div className='flex items-center border-border/40 border-t px-5 py-2.5'>
            <span className='text-[11px] text-muted-foreground/50'>Created {createdAt}</span>
          </div>
        )}
      </button>
    </li>
  );
}
