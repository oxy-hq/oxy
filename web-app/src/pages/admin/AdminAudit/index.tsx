import { useEffect, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useAuditSearch } from "@/hooks/api/audit";
import AuditTable from "./components/AuditTable";

const LIMIT = 200;

/** Debounce a fast-changing string (e.g. a search box) by `ms`. */
function useDebounced(value: string, ms = 300): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return debounced;
}

/**
 * Platform audit log (`/admin/audit`, Oxy staff). Free-text search + action /
 * outcome facets over the append-only `audit_events` stream, newest first.
 */
export default function AdminAudit() {
  const [qInput, setQInput] = useState("");
  const [actionInput, setActionInput] = useState("");
  const [outcome, setOutcome] = useState("all");

  const q = useDebounced(qInput);
  const action = useDebounced(actionInput);

  const { data, isPending, isError } = useAuditSearch({
    q: q || undefined,
    action: action || undefined,
    outcome: outcome === "all" ? undefined : outcome,
    limit: LIMIT
  });

  const hasFilters = !!q || !!action || outcome !== "all";
  const clear = () => {
    setQInput("");
    setActionInput("");
    setOutcome("all");
  };

  return (
    <div className='mx-auto w-full max-w-[100rem] space-y-4 p-6'>
      <div>
        <h1 className='font-semibold text-xl tracking-tight'>Audit log</h1>
        <p className='text-muted-foreground text-xs'>
          Every privileged action across the platform — partner grants, member changes, custom-app
          deploys — newest first.
        </p>
      </div>

      <div className='flex flex-wrap items-center gap-2'>
        <Input
          placeholder='Search action, actor, or target…'
          value={qInput}
          onChange={(e) => setQInput(e.target.value)}
          className='max-w-xs'
        />
        <Input
          placeholder='Action (e.g. partner.member.added)'
          value={actionInput}
          onChange={(e) => setActionInput(e.target.value)}
          className='max-w-xs'
        />
        <Select value={outcome} onValueChange={setOutcome}>
          <SelectTrigger className='w-36'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value='all'>All outcomes</SelectItem>
            <SelectItem value='success'>Success</SelectItem>
            <SelectItem value='failure'>Failure</SelectItem>
          </SelectContent>
        </Select>
        {hasFilters && (
          <Button variant='ghost' size='sm' onClick={clear}>
            Clear
          </Button>
        )}
      </div>

      <AuditTable events={data} isPending={isPending} isError={isError} limit={LIMIT} />
    </div>
  );
}
