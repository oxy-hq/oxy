import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { SCOPE_LABELS } from "@/libs/operatingGraph";
import type { RoleRow, RoleScope } from "@/types/operatingGraph";

/** The org's position vocabulary, optionally narrowed to one scope. */
export function PositionSelect({
  roles,
  value,
  onValueChange,
  scope,
  placeholder = "Pick a position",
  id,
  disabled,
  testId
}: {
  roles: RoleRow[];
  value: string;
  onValueChange: (value: string) => void;
  /** Only offer positions of this scope; omit for all, with the scope shown. */
  scope?: RoleScope;
  placeholder?: string;
  id?: string;
  disabled?: boolean;
  testId: string;
}) {
  const offered = scope ? roles.filter((r) => r.scope === scope) : roles;
  return (
    <Select value={value} onValueChange={onValueChange} disabled={disabled || offered.length === 0}>
      <SelectTrigger id={id} className='w-full' data-testid={testId}>
        <SelectValue placeholder={offered.length === 0 ? "No positions yet" : placeholder} />
      </SelectTrigger>
      <SelectContent>
        {offered.map((role) => (
          <SelectItem key={role.id} value={role.id}>
            {role.name}
            {!scope && (
              <span className='ml-2 text-muted-foreground text-xs'>{SCOPE_LABELS[role.scope]}</span>
            )}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
