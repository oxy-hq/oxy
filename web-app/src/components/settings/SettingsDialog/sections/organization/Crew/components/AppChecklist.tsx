import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { Label } from "@/components/ui/shadcn/label";
import type { AppAccessSummary } from "@/types/appAccess";

/**
 * Which of the org's custom apps a worker may open.
 *
 * A plain checklist rather than the teams-and-people grant picker: a worker
 * belongs to no team, and every app the org has is a candidate, so there is
 * nothing to search for.
 */
export function AppChecklist({
  apps,
  selected,
  onChange,
  idPrefix
}: {
  apps: AppAccessSummary[];
  selected: string[];
  onChange: (next: string[]) => void;
  /** Keeps checkbox ids unique when two dialogs render the same list. */
  idPrefix: string;
}) {
  if (apps.length === 0) {
    return (
      <p className='rounded-md border border-dashed px-3 py-2 text-muted-foreground text-xs'>
        No custom apps are published to this organization yet. Apps can be granted later.
      </p>
    );
  }

  const toggle = (appId: string, checked: boolean) =>
    onChange(checked ? [...selected, appId] : selected.filter((id) => id !== appId));

  return (
    <ul className='max-h-48 divide-y overflow-y-auto rounded-md border'>
      {apps.map((app) => {
        const id = `${idPrefix}-${app.id}`;
        return (
          <li key={app.id} className='flex items-center gap-2.5 px-3 py-2'>
            <Checkbox
              id={id}
              checked={selected.includes(app.id)}
              onCheckedChange={(checked) => toggle(app.id, checked === true)}
            />
            <Label
              htmlFor={id}
              className='flex min-w-0 flex-1 cursor-pointer items-baseline gap-2 font-normal'
            >
              <span className='truncate text-sm'>{app.name}</span>
              {!app.published && (
                <span className='shrink-0 text-muted-foreground text-xs'>not published</span>
              )}
            </Label>
          </li>
        );
      })}
    </ul>
  );
}
