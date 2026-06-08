import { VersionBadge } from "@/components/settings/SettingsDialog/components/VersionBadge";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator
} from "@/components/ui/shadcn/breadcrumb";
import { Separator } from "@/components/ui/shadcn/separator";
import { SidebarTrigger } from "@/components/ui/shadcn/sidebar";
import { AdminEntitySearch } from "../../components/AdminEntitySearch";

interface AdminTopbarProps {
  title: string;
}

export function AdminTopbar({ title }: AdminTopbarProps) {
  // VersionBadge on the right gives operators a one-glance answer to
  // "which oxy build am I looking at?" — critical when triaging a
  // staging vs prod regression without having to scroll to /version
  // or open a settings dialog. Reuses the existing VersionBadge
  // popover (build hash, profile, commit link) so we don't duplicate
  // the build-info surface.
  return (
    <header className='flex h-12 shrink-0 items-center gap-2 border-b bg-background px-4'>
      <SidebarTrigger className='-ml-1' />
      <Separator orientation='vertical' className='mr-2 h-4' />
      <Breadcrumb>
        <BreadcrumbList>
          <BreadcrumbItem className='text-muted-foreground'>Admin</BreadcrumbItem>
          <BreadcrumbSeparator />
          <BreadcrumbItem>
            <BreadcrumbPage>{title}</BreadcrumbPage>
          </BreadcrumbItem>
        </BreadcrumbList>
      </Breadcrumb>
      <div className='ml-auto flex items-center gap-3'>
        <AdminEntitySearch />
        <Separator orientation='vertical' className='h-4' />
        <VersionBadge />
      </div>
    </header>
  );
}
