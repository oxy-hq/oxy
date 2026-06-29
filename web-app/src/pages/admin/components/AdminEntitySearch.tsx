import { Building2, FolderOpen, MessageSquare, Play, Search, User } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator
} from "@/components/ui/shadcn/command";
import { useExplorerRuns, useExplorerThreads } from "@/hooks/api/adminExplorer";
import { useAdminOrgsList } from "@/hooks/api/adminTenants/useAdminOrgs";
import { useAdminUsersList } from "@/hooks/api/adminTenants/useAdminUsers";
import { useAdminWorkspacesList } from "@/hooks/api/adminTenants/useAdminWorkspaces";
import ROUTES from "@/libs/utils/routes";

/**
 * Cmd+K / Ctrl+K universal search across orgs, users, and workspaces.
 * The operator-glue that turns three siloed lists into one navigable
 * graph: type "alice" → see Alice's user row → jump straight to her
 * detail page, no detour through `/admin/users`. Then from that page,
 * Cmd+K → "acme" → straight to Acme's org page.
 *
 * Each result group is bordered by a section divider so the operator can
 * quickly distinguish entity types when results are crowded. The trigger
 * pill is the visible affordance; the dialog opens on click or on Cmd+K.
 */
export const AdminEntitySearch = () => {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const navigate = useNavigate();

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const isToggle = (event.key === "k" || event.key === "K") && (event.metaKey || event.ctrlKey);
      if (isToggle) {
        event.preventDefault();
        setOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // Reset query on close so the next open starts clean.
  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  // The list endpoints already support a `search` query param. We pass the
  // raw input through and let the backend do the heavy lifting; the
  // CommandDialog's built-in cmdk filter then does a final pass on the
  // returned rows so even a slow round-trip feels responsive.
  const orgsQuery = useAdminOrgsList({ search: query }, { enabled: open });
  const usersQuery = useAdminUsersList({ search: query }, { enabled: open });
  const workspacesQuery = useAdminWorkspacesList({ search: query }, { enabled: open });
  // Only search threads/runs once the operator has typed something — an
  // unfiltered cross-tenant scan on every palette-open isn't worth the round
  // trip.
  const threadsQuery = useExplorerThreads({ search: query }, { enabled: open && query.length > 1 });
  const runsQuery = useExplorerRuns({ search: query }, { enabled: open && query.length > 1 });

  const orgs = useMemo(() => orgsQuery.data?.slice(0, 6) ?? [], [orgsQuery.data]);
  const users = useMemo(() => usersQuery.data?.slice(0, 6) ?? [], [usersQuery.data]);
  const workspaces = useMemo(() => workspacesQuery.data?.slice(0, 6) ?? [], [workspacesQuery.data]);
  const threads = useMemo(() => threadsQuery.data?.items.slice(0, 6) ?? [], [threadsQuery.data]);
  const runs = useMemo(() => runsQuery.data?.items.slice(0, 6) ?? [], [runsQuery.data]);

  const go = (path: string) => {
    setOpen(false);
    navigate(path);
  };

  return (
    <>
      <Button
        variant='outline'
        size='sm'
        onClick={() => setOpen(true)}
        className='h-8 gap-2 px-2.5 text-muted-foreground'
      >
        <Search className='size-3.5' />
        <span className='hidden text-xs sm:inline'>Search tenants</span>
        <kbd className='ml-1 hidden items-center gap-0.5 rounded border border-border/60 bg-muted/40 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground sm:inline-flex'>
          <span className='text-xs'>⌘</span>K
        </kbd>
      </Button>

      <CommandDialog
        open={open}
        onOpenChange={setOpen}
        title='Tenant search'
        description='Find orgs, users, and workspaces across the deployment.'
      >
        <CommandInput
          placeholder='Search orgs, users, workspaces…'
          value={query}
          onValueChange={setQuery}
        />
        <CommandList>
          <CommandEmpty>
            {query.length === 0
              ? "Start typing to search across the tenant graph."
              : "No matches across orgs, users, or workspaces."}
          </CommandEmpty>

          {orgs.length > 0 ? (
            <>
              <CommandGroup heading='Organizations'>
                {orgs.map((org) => (
                  <CommandItem
                    key={`org-${org.id}`}
                    value={`org ${org.name} ${org.slug} ${org.owner_email ?? ""}`}
                    onSelect={() => go(ROUTES.ADMIN.ORG_DETAIL(org.id))}
                  >
                    <Building2 className='size-4 text-muted-foreground' />
                    <span className='flex-1 truncate'>{org.name}</span>
                    <span className='font-mono text-[11px] text-muted-foreground'>/{org.slug}</span>
                    <span className='hidden text-muted-foreground text-xs tabular-nums sm:inline'>
                      {org.member_count} mbr · {org.workspace_count} ws
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
              <CommandSeparator />
            </>
          ) : null}

          {users.length > 0 ? (
            <>
              <CommandGroup heading='Users'>
                {users.map((user) => (
                  <CommandItem
                    key={`user-${user.id}`}
                    value={`user ${user.name} ${user.email}`}
                    onSelect={() => go(ROUTES.ADMIN.USER_DETAIL(user.id))}
                  >
                    <User className='size-4 text-muted-foreground' />
                    <span className='flex-1 truncate'>{user.name || user.email}</span>
                    {user.name ? (
                      <span className='truncate font-mono text-[11px] text-muted-foreground'>
                        {user.email}
                      </span>
                    ) : null}
                    <span className='hidden text-muted-foreground text-xs tabular-nums sm:inline'>
                      {user.org_count} org{user.org_count === 1 ? "" : "s"}
                    </span>
                  </CommandItem>
                ))}
              </CommandGroup>
              <CommandSeparator />
            </>
          ) : null}

          {workspaces.length > 0 ? (
            <CommandGroup heading='Workspaces'>
              {workspaces.map((ws) => (
                <CommandItem
                  key={`ws-${ws.id}`}
                  value={`workspace ${ws.name} ${ws.org_slug ?? ""}`}
                  onSelect={() => go(ROUTES.ADMIN.WORKSPACE_DETAIL(ws.id))}
                >
                  <FolderOpen className='size-4 text-muted-foreground' />
                  <span className='flex-1 truncate'>{ws.name}</span>
                  {ws.org_slug ? (
                    <span className='font-mono text-[11px] text-muted-foreground'>
                      /{ws.org_slug}
                    </span>
                  ) : null}
                  <span className='hidden text-muted-foreground text-xs tabular-nums sm:inline'>
                    {ws.member_count} mbr
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          ) : null}

          {threads.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading='Threads'>
                {threads.map((t) => {
                  const openable = t.org_slug && t.workspace_id;
                  return (
                    <CommandItem
                      key={`thread-${t.id}`}
                      value={`thread ${t.title} ${t.workspace_name ?? ""} ${t.org_name ?? ""}`}
                      disabled={!openable}
                      onSelect={() => {
                        if (openable) {
                          go(
                            ROUTES.ORG(t.org_slug as string)
                              .WORKSPACE(t.workspace_id as string)
                              .THREAD(t.id)
                          );
                        }
                      }}
                    >
                      <MessageSquare className='size-4 text-muted-foreground' />
                      <span className='flex-1 truncate'>{t.title || "(untitled)"}</span>
                      {t.workspace_name ? (
                        <span className='hidden truncate font-mono text-[11px] text-muted-foreground sm:inline'>
                          {t.workspace_name}
                        </span>
                      ) : null}
                    </CommandItem>
                  );
                })}
              </CommandGroup>
            </>
          ) : null}

          {runs.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading='Runs'>
                {runs.map((r) => {
                  const openable = r.org_slug && r.workspace_id && r.thread_id;
                  return (
                    <CommandItem
                      key={`run-${r.id}`}
                      value={`run ${r.question_snippet} ${r.workspace_name ?? ""} ${r.org_name ?? ""}`}
                      disabled={!openable}
                      onSelect={() => {
                        if (openable) {
                          go(
                            ROUTES.ORG(r.org_slug as string)
                              .WORKSPACE(r.workspace_id as string)
                              .THREAD(r.thread_id as string)
                          );
                        }
                      }}
                    >
                      <Play className='size-4 text-muted-foreground' />
                      <span className='flex-1 truncate'>
                        {r.question_snippet || "(no question)"}
                      </span>
                      {r.task_status ? (
                        <span className='hidden font-mono text-[11px] text-muted-foreground sm:inline'>
                          {r.task_status}
                        </span>
                      ) : null}
                    </CommandItem>
                  );
                })}
              </CommandGroup>
            </>
          ) : null}
        </CommandList>
      </CommandDialog>
    </>
  );
};
