import { ShieldCheck } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import type { OrgRole } from "@/types/organization";

export function RoleBadge({ role }: { role: OrgRole }) {
  if (role === "owner") {
    return (
      <Badge
        variant='outline'
        className='gap-1 border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
      >
        <ShieldCheck className='h-3 w-3' />
        Owner
      </Badge>
    );
  }
  if (role === "admin") {
    return (
      <Badge variant='outline' className='gap-1 border-primary/30 bg-primary/5 text-primary'>
        <ShieldCheck className='h-3 w-3' />
        Admin
      </Badge>
    );
  }
  return (
    <Badge variant='outline' className='text-muted-foreground'>
      Member
    </Badge>
  );
}
